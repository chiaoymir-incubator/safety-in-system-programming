use crossbeam_channel;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let len = input_vec.len();
    let mut output_vec: Vec<U> = Vec::with_capacity(len);
    unsafe { output_vec.set_len(len); }

    let (sender_in, receiver_in): (crossbeam_channel::Sender<(usize, T)>, crossbeam_channel::Receiver<(usize, T)>) = crossbeam_channel::unbounded();
    let (sender_out, receiver_out): (crossbeam_channel::Sender<(usize, U)>, crossbeam_channel::Receiver<(usize, U)>) = crossbeam_channel::unbounded();

    let mut threads= Vec::new();
    for _ in 0..num_threads {
        let receiver = receiver_in.clone();
        let sender = sender_out.clone();
        threads.push(thread::spawn(move || {
            while let Ok((i, v)) = receiver.recv() {
                let result = f(v);
                sender.send((i, result)).expect("Tried writing to the channel, but there are no receivers!");
            }
        }));
    }
    
    let sender_in_ref = sender_in.clone();
    let thread_send = thread::spawn(move || {
        while let Some(v) = input_vec.pop() {
            let i = input_vec.len();
            sender_in_ref.send((i, v)).expect("Tried writing to the channel, but there are no receivers!");
        }
    });

    let mut counter = 0;
    while counter < len {
        if let Ok((i, v)) = receiver_out.recv() {
            output_vec[i] = v;
            counter += 1;
        } else {
            panic!("something went wrong!");
        }   
    }

    drop(sender_in);
    drop(sender_out);

    thread_send.join().expect("Panic occured in thread!");
    for thread in threads {
        thread.join().expect("Panic occured in thread!");
    }

    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
