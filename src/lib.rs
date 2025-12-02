// use std::{
//     sync::{Arc, Mutex, mpsc},
//     thread,
// };

// pub struct ThreadPool {
//     workers: Vec<Worker>,
//     sender: Option<mpsc::Sender<Job>>,
// }

// type Job = Box<dyn FnOnce() + Send + 'static>;

// impl ThreadPool {
//     pub fn new(size: usize) -> ThreadPool {
//         assert!(size > 0);
//         let (sender, receiver) = mpsc::channel();
//         let receiver = Arc::new(Mutex::new(receiver));
//         let mut workers = Vec::with_capacity(size);
//         for id in 0..size {
//             workers.push(Worker::new(id, Arc::clone(&receiver)));
//         }
//         ThreadPool {
//             workers,
//             sender: Some(sender),
//         }
//     }
//     pub fn execute<F>(&self, f: F)
//     where
//         F: FnOnce() -> () + Send + 'static,
//     {
//         let job = Box::new(f);
//         self.sender.as_ref().unwrap().send(job).unwrap();
//     }
// }

// impl Drop for ThreadPool {
//     fn drop(&mut self) {
//         drop(self.sender.take());
//         for worker in &mut self.workers {
//             println!("Shutting down worker {}", worker.id);
//             if let Some(thread) = worker.thread.take() {
//                 thread.join().unwrap();
//             }
//         }
//     }
// }
// struct Worker {
//     id: usize,
//     thread: Option<thread::JoinHandle<()>>,
// }
// impl Worker {
//     fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
//         let thread = thread::spawn(move || {
//             loop {
//                 let message = receiver.lock().unwrap().recv();
//                 match message {
//                     Ok(job) => {
//                         println!("Woker {id} got a job; executing.");
//                         job();
//                     }
//                     Err(_) => {
//                         println!("Worker {id} disconnected: shutting down.");
//                         break;
//                     }
//                 }
//             }
//         });

//         Worker {
//             id,
//             thread: Some(thread),
//         }
//     }
// }

#[cfg(feature = "async-pool")]
mod async_impl {
    use futures::StreamExt;
    use futures::channel::mpsc;
    use futures::executor::ThreadPool;
    use futures::task::SpawnExt;
    use std::future::Future;
    use std::pin::Pin;

    pub struct AsyncPool {
        executor: ThreadPool,
        sender: mpsc::UnboundedSender<Job>,
    }

    type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

    impl AsyncPool {
        pub fn new(size: usize) -> Self {
            assert!(size > 0);

            let executor = ThreadPool::builder()
                .pool_size(size)
                .create()
                .expect("Failed to create thread pool");

            let (sender, mut receiver) = mpsc::unbounded::<Job>();

            let executor_clone = executor.clone();
            executor
                .spawn(async move {
                    while let Some(job) = receiver.next().await {
                        executor_clone.spawn(job).ok();
                    }
                })
                .ok();

            AsyncPool { executor, sender }
        }

        pub fn execute<F>(&self, future: F)
        where
            F: Future<Output = ()> + Send + 'static,
        {
            let job = Box::pin(future);
            let _ = self.sender.unbounded_send(job);
        }

        pub fn execute_with_retry<F, Fut>(&self, f: F, max_retries: usize)
        where
            F: Fn() -> Fut + Send + 'static,
            Fut: Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + 'static,
        {
            let sender = self.sender.clone();
            let future = async move {
                for attempt in 0..=max_retries {
                    match f().await {
                        Ok(_) => return,
                        Err(e) if attempt < max_retries => {
                            eprintln!("Attempt {} failed: {}. Retrying...", attempt + 1, e);
                            continue;
                        }
                        Err(e) => {
                            eprintln!("All {} attempts failed. Last error: {}", max_retries + 1, e);
                            return;
                        }
                    }
                }
            };
            let _ = sender.unbounded_send(Box::pin(future));
        }
    }

    impl Drop for AsyncPool {
        fn drop(&mut self) {
            // Keep executor alive until drop to ensure cleanup
            let _ = &self.executor;
        }
    }
}

#[cfg(not(feature = "async-pool"))]
mod sync_impl {
    use std::{
        sync::{Arc, Mutex, mpsc},
        thread,
    };

    pub struct ThreadPool {
        workers: Vec<Worker>,
        sender: Option<mpsc::Sender<Job>>,
    }

    type Job = Box<dyn FnOnce() + Send + 'static>;

    impl ThreadPool {
        pub fn new(size: usize) -> ThreadPool {
            assert!(size > 0);
            let (sender, receiver) = mpsc::channel();
            let receiver = Arc::new(Mutex::new(receiver));
            let mut workers = Vec::with_capacity(size);
            for id in 0..size {
                workers.push(Worker::new(id, Arc::clone(&receiver)));
            }
            ThreadPool {
                workers,
                sender: Some(sender),
            }
        }

        pub fn execute<F>(&self, f: F)
        where
            F: FnOnce() -> () + Send + 'static,
        {
            let job = Box::new(f);
            self.sender.as_ref().unwrap().send(job).unwrap();
        }
    }

    impl Drop for ThreadPool {
        fn drop(&mut self) {
            drop(self.sender.take());
            for worker in &mut self.workers {
                println!("Shutting down worker {}", worker.id);
                if let Some(thread) = worker.thread.take() {
                    thread.join().unwrap();
                }
            }
        }
    }

    struct Worker {
        id: usize,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl Worker {
        fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
            let thread = thread::spawn(move || {
                loop {
                    let message = receiver.lock().unwrap().recv();
                    match message {
                        Ok(job) => {
                            println!("Worker {id} got a job; executing.");
                            job();
                        }
                        Err(_) => {
                            println!("Worker {id} disconnected: shutting down.");
                            break;
                        }
                    }
                }
            });

            Worker {
                id,
                thread: Some(thread),
            }
        }
    }
}

// Public API
#[cfg(feature = "async-pool")]
pub use async_impl::AsyncPool as Pool;

#[cfg(not(feature = "async-pool"))]
pub use sync_impl::ThreadPool as Pool;

// Example usage:
// let pool = AsyncPool::new(4);
// pool.execute(async {
//     println!("Task executed!");
// });
