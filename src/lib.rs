#[macro_use]
extern crate downcast_rs;
extern crate stopwatch;
extern crate num_cpus;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate erased_serde;
#[allow(unused_imports)]
#[macro_use]
extern crate serde_derive;

pub mod fungine {
    use std::thread;
    use std::sync::Arc;
    use std::sync::mpsc::Sender;
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc;
    use std::net::UdpSocket;
    use std::collections::BTreeMap as Map;
    use std::io::BufWriter;
    use std::ops::Deref;
    use downcast_rs::Downcast;
    use num_cpus;
    use serde_json;
    use erased_serde;
    use stopwatch::{Stopwatch};

    // A struct representing a message between GameObjects. 
    // This will probably change into a trait once in use.
    pub struct Message {

    }

    // The GameObject trait will be implemented by everything that wants to be executed
    // as part of a frame step by the engine.
    pub trait GameObject: Downcast + Send + Sync + erased_serde::Serialize {
        fn update(&self, current_state: Arc<Vec<Arc<Box<GameObject>>>>, messages: Vec<Message>) -> Box<GameObject>;
        fn box_clone(&self) -> Box<GameObject>;
    }
    impl_downcast!(GameObject);
    serialize_trait_object!(GameObject);

    impl Clone for Box<GameObject>
    {
        fn clone(&self) -> Box<GameObject> {
            self.box_clone()
        }
    }

    #[derive(Clone)]
    pub struct GameObjectWithState(Arc<Box<GameObject>>, Arc<Vec<Arc<Box<GameObject>>>>);

    // The main engine structure. This stores the state, communications and networking objects.
    pub struct Fungine {
        initial_state: Arc<Vec<Arc<Box<GameObject>>>>,
        sends: Vec<Sender<GameObjectWithState>>,
        receiver: Receiver<Arc<Box<GameObject>>>,
        current_state: Arc<Vec<Arc<Box<GameObject>>>>
    }

    impl Fungine {
        // Constructor that sets up the engine with an initial state, worker threads and networking.
        pub fn new(initial_state: Arc<Vec<Arc<Box<GameObject>>>>, port: Option<String>) -> Fungine {
            // Create channels for sending objects to process to worker threads.
            let (send_modified, receive_modified) = mpsc::channel();
            let receiver = receive_modified;
            let mut sends = vec![];
            // Set up networking (if a port is passed)
            let socket: Option<UdpSocket> = match port.clone() {
                Some(_) => {
                    let soc = UdpSocket::bind("0.0.0.0:0");
                    match soc {
                        Ok(s) => Some(s),
                        Err(_) => panic!("Couldn't bind send port")
                    }
                },
                None => None
            };
            // Create channel for sending object to UDP worker thread.
            let send_tx = match socket {
                Some(socket) => {
                    let (s_tx, r_tx) = mpsc::channel();
                    thread::spawn(move || {
                        let mut sw = Stopwatch::start_new();
                        let mut sent_count = 0;
                        loop {
                            if sw.elapsed_ms() > 10000 {
                                println!("State sends per second: {}", sent_count / 10);
                                sent_count = 0;
                                sw.restart();
                            }
                            match r_tx.recv() {
                                Ok(state) => {
                                    let state: Arc<Box<GameObject>> = state;
                                    // Send over networking
                                    if let Some(p) = port.clone() {
                                        let mut buf = Vec::new();
                                        {
                                            let buf = &mut buf;
                                            let writer = BufWriter::new(buf);
                                            let json = &mut serde_json::ser::Serializer::new(writer);
                                            let mut formats: Map<&str, Box<erased_serde::Serializer>> = Map::new();
                                            formats.insert("json", Box::new(erased_serde::Serializer::erase(json)));
                                            let mut values: Map<&str, Box<GameObject>> = Map::new();
                                            values.insert("state", state.deref().box_clone());
                                            let format = formats.get_mut("json").unwrap();
                                            let value = &values["state"];
                                            value.erased_serialize(format).unwrap();
                                        }
                                        
                                        let addr: String = format!("127.0.0.1:{}", p);
                                        let payload = buf.as_slice();

                                        match socket.send_to(payload, addr) {
                                            Ok(_) => {},
                                            Err(e) => println!("Failed to send: {}", e)
                                        }
                                    }
                                },
                                Err(_) => {
                                    // The channel has been closed so exit the worker
                                    println!("Closing UDP thread");
                                    break;
                                }
                            }
                            sent_count += 1;
                        }
                    });
                    Some(s_tx)
                },
                None => None
            };
            // Create worker threads
            let thread_count = num_cpus::get() - 2;
            for _ in 0..thread_count {
                let send_modified = send_modified.clone();
                let s_tx = send_tx.clone();
                let (send_original, receive_original) = mpsc::channel();
                sends.push(send_original);

                thread::spawn(move || {
                    // A worker thread will pull GameObjects, execute their update method and send the new
                    // state back to the main thread.
                    loop {
                        let s_tx = s_tx.clone();
                        match receive_original.recv() {
                            Ok(original) => {
                                let original: GameObjectWithState = original;
                                let current_object: Arc<Box<GameObject>> = Arc::clone(&original.0);
                                let current_state: Arc<Vec<Arc<Box<GameObject>>>> = Arc::clone(&original.1);
                                let messages: Vec<Message> = Vec::new();
                                let new = current_object.update(current_state, messages);
                                let new_ref = Arc::new(new);
                                send_modified.send(Arc::clone(&new_ref)).unwrap();
                                if let Some(tx) = s_tx {
                                    tx.send(new_ref).unwrap();
                                }
                            },
                            Err(_) => {
                                // The channel has been closed so exit the worker
                                println!("Closing worker thread");
                                break;
                            }
                        }
                    }
                });
            }

            Fungine {
                initial_state: initial_state.clone(),
                sends: sends,
                receiver: receiver,
                current_state: initial_state.clone()
            }
        }

        // Step the engine forward indefinitely.
        pub fn run(&self) {
            let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state.clone();
            let mut sw = Stopwatch::start_new();
            let mut frame_count = 0;

            loop {
                if sw.elapsed_ms() > 10000 {
                    println!("Frames processed per second: {}", frame_count / 10);
                    sw.restart();
                    frame_count = 0;
                }
                states = Fungine::step_engine(&states, &self.sends, &self.receiver);
                frame_count += 1;
            }
        }

        // Step the engine forward a specified number of steps, used for testing.
        pub fn run_steps(&self, steps: u32) -> Arc<Vec<Arc<Box<GameObject>>>> {
            let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state.clone();

            for _ in 0..steps {
                states = Fungine::step_engine(&states, &self.sends, &self.receiver);
            }

            states
        }

        // Step the engine forward a specified number of steps from a provided state.
        pub fn run_steps_cont(&mut self, steps: u32) -> Arc<Vec<Arc<Box<GameObject>>>> {
            let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.current_state.clone();

            for _ in 0..steps {
                states = Fungine::step_engine(&states, &self.sends, &self.receiver);
            }

            self.current_state = states.clone();

            states
        }

        // Perform one step by processing each GameObject in the state once.
        fn step_engine(states: &Arc<Vec<Arc<Box<GameObject>>>>, sends: &[Sender<GameObjectWithState>], receiver: &Receiver<Arc<Box<GameObject>>>) -> Arc<Vec<Arc<Box<GameObject>>>> {
            // Send current states to the worker threads
            for x in 0..states.len() {
                let states = Arc::clone(states);
                let state = Arc::clone(&states[x]);
                sends[x % sends.len()].send(GameObjectWithState(state, states)).unwrap();
            }
            let mut next_states: Vec<Arc<Box<GameObject>>> = vec![];
            // Collect new states
            for _ in 0..states.len() {
                let state = receiver.recv().unwrap();
                next_states.push(state);
            }
            Arc::new(next_states)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::net::UdpSocket;
    use std::str;
    use std::str::FromStr;
    use fungine::{ Fungine, GameObject, Message };
    use stopwatch::{Stopwatch};
    use serde_json;

    // A GameObject implementation with some state
    #[derive(Clone, Serialize, Deserialize, Debug)]
    struct TestGameObject {
        value: i32,
    }

    impl GameObject for TestGameObject {
        fn box_clone(&self) -> Box<GameObject> {
            Box::new((*self).clone())
        }

        // The update function simply increments the internal counter
        fn update(&self, _current_state: Arc<Vec<Arc<Box<GameObject>>>>, _messages: Vec<Message>) -> Box<GameObject> {
            Box::new(TestGameObject {
                value: self.value + 1
            })
        }
    }
    unsafe impl Send for TestGameObject {}
    unsafe impl Sync for TestGameObject {}


    // A single object, single iteration test to make sure state is passed around correctly
    #[test]
    fn test_iterate() {
        let initial_object = TestGameObject {
            value: 0
        };
        let initial_object = Box::new(initial_object) as Box<GameObject>;
        let initial_object = Arc::new(initial_object);
        let initial_state = Arc::new(vec![initial_object]);
        let engine = Fungine::new(initial_state, None);
        let next_states = engine.run_steps(1);
        let next_state = next_states[0].clone();
        let next_state: Box<GameObject> = next_state.box_clone();
        if let Some(next_object) = next_state.downcast_ref::<TestGameObject>() {
            assert_eq!(1, next_object.value);
        }
        else {
            assert!(false);
        }
    }

    // Run 1000 steps on 1000 boids to check state and benchmark speed
    #[test]
    fn speed_test() {
        let mut initial_state = Vec::new();
        for _ in 0..1000 {
            let initial_object = TestGameObject {
                value: 0
            };
            let initial_object = Box::new(initial_object) as Box<GameObject>;
            let initial_object = Arc::new(initial_object);
            initial_state.push(initial_object);
        }
        let engine = Fungine::new(Arc::new(initial_state), None);
        let sw = Stopwatch::start_new();
        let final_states = engine.run_steps(1000);
        println!("Time taken: {}ms", sw.elapsed_ms());
        for x in 0..final_states.len() {
            let final_state = final_states[x].clone();
            let final_state: Box<GameObject> = final_state.box_clone();
            if let Some(object) = final_state.downcast_ref::<TestGameObject>() {
                assert_eq!(1000, object.value);
            }
            else {
                assert!(false);
            }
        }
    }

    // Test that state can be sent over UDP and correct objects are received
    #[test]
    fn network_test() {
        let initial_object = TestGameObject {
            value: 0
        };
        let mut buf = [0; 11];
        let port = "4794";
        let soc = UdpSocket::bind(format!("127.0.0.1:{}", port));
        let socket = match soc {
            Ok(s) => s,
            Err(_) => panic!("Couldn't open listen socket")
        };
        let initial_object = Box::new(initial_object) as Box<GameObject>;
        let initial_object = Arc::new(initial_object);
        let initial_state = Arc::new(vec![initial_object]);
        let engine = Fungine::new(initial_state, Some(String::from_str(port)).unwrap().ok());
        let final_states = engine.run_steps(1);
        let amt = match socket.recv_from(&mut buf)
        {
            Ok((a, _)) => a,
            Err(_) => panic!("Couldn't receive")
        };
        assert_eq!(11, amt);

        let serialized = str::from_utf8(&buf).unwrap();
        let serialized = &serialized[..amt];
        let deserialized: TestGameObject = serde_json::from_str(&serialized).unwrap();
        assert_eq!(1, final_states.len());
        for x in 0..final_states.len() {
            let final_state = final_states[x].clone();
            if let Some(object) = final_state.downcast_ref::<TestGameObject>() {
                assert_eq!(object.value, deserialized.value);
            }
            else {
                assert!(false);
            }
        }
    }

    // Run 1000 steps on 1000 boids to check state and benchmark speed
    #[test]
    fn speed_with_networking_test() {
        let mut initial_state = Vec::new();
        for _ in 0..1000 {
            let initial_object = TestGameObject {
                value: 0
            };
            let initial_object = Box::new(initial_object) as Box<GameObject>;
            let initial_object = Arc::new(initial_object);
            initial_state.push(initial_object);
        }
        let port = "4795";
        let soc = UdpSocket::bind(format!("127.0.0.1:{}", port));
        let socket = match soc {
            Ok(s) => s,
            Err(_) => panic!("Couldn't open listen socket")
        };
        let final_states;
        {
            let engine = Fungine::new(Arc::new(initial_state), Some(String::from_str(port)).unwrap().ok());
            let sw = Stopwatch::start_new();
            final_states = engine.run_steps(1000);
            println!("Time taken: {}ms", sw.elapsed_ms());
        }
        assert_eq!(1000, final_states.len());
        for x in 0..final_states.len() {
            let final_state = final_states[x].clone();
            let final_state: Box<GameObject> = final_state.box_clone();
            if let Some(object) = final_state.downcast_ref::<TestGameObject>() {
                assert_eq!(1000, object.value);
            }
            else {
                assert!(false);
            }
        }
        let mut buf = [0; 11];
        let amt = match socket.recv_from(&mut buf)
        {
            Ok((a, _)) => a,
            Err(_) => panic!("Couldn't receive")
        };
        assert_eq!(11, amt);
    }
}