#[macro_use]
extern crate downcast_rs;
extern crate stopwatch;
extern crate num_cpus;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate erased_serde;

pub mod fungine {
    use std::thread;
    use std::sync::Arc;
    use std::sync::mpsc::Sender;
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc;
    use std::net::UdpSocket;
    use std::collections::BTreeMap as Map;
    use std::io;
    use std::ops::Deref;
    use downcast_rs::Downcast;
    use num_cpus;
    use serde_json;
    use serde::Serialize;
    use erased_serde;

    pub struct Message {

    }

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

    pub struct Fungine {
        initial_state: Arc<Vec<Arc<Box<GameObject>>>>,
        sends: Vec<Sender<(Arc<Box<GameObject>>, Arc<Vec<Arc<Box<GameObject>>>>)>>,
        receiver: Receiver<Arc<Box<GameObject>>>,
        socket: Option<UdpSocket>,
        port: Option<String>
    }

    impl Fungine {
        pub fn new(initial_state: Arc<Vec<Arc<Box<GameObject>>>>, port: Option<String>) -> Fungine {
            let (send_modified, receive_modified) = mpsc::channel();
            let receiver = receive_modified;
            let mut sends = vec![];
            let socket: Option<UdpSocket> = match port.clone() {
                Some(_) => {
                    let soc = UdpSocket::bind("127.0.0.1:0");
                    match soc {
                        Ok(s) => Some(s),
                        Err(_) => None
                    }
                },
                None => None
            };
            for _ in 0..num_cpus::get() {
                let send_modified = send_modified.clone();
                let (send_original, receive_original) = mpsc::channel();
                sends.push(send_original);

                thread::spawn(move || {
                    loop {
                        match receive_original.recv() {
                            Ok(original) => {
                                let original: (Arc<Box<GameObject>>, Arc<Vec<Arc<Box<GameObject>>>>) = original;
                                let current_object: Arc<Box<GameObject>> = original.clone().0.clone();
                                let current_state: Arc<Vec<Arc<Box<GameObject>>>> = original.clone().1.clone();
                                let messages: Vec<Message> = Vec::new();
                                let new = current_object.update(current_state, messages);
                                send_modified.send(Arc::new(new)).unwrap();
                            },
                            Err(_) => {
                                println!("Closing worker thread");
                                break;
                            }
                        }
                    }
                });
            }

            Fungine {
                initial_state: initial_state,
                sends: sends,
                receiver: receiver,
                socket: socket,
                port: port
            }
        }

        pub fn run(self) {
            let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state;

            loop {
                states = Fungine::step_engine(&self.socket, self.port.clone(), states, self.sends.clone(), &self.receiver);
            }
        }

        pub fn run_steps(self, steps: u32) -> Arc<Vec<Arc<Box<GameObject>>>> {
            let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state;

            for _ in 0..steps {
                states = Fungine::step_engine(&self.socket, self.port.clone(), states, self.sends.clone(), &self.receiver);
            }

            states
        }

        fn step_engine(socket: &Option<UdpSocket>, port: Option<String>, states: Arc<Vec<Arc<Box<GameObject>>>>, sends: Vec<Sender<(Arc<Box<GameObject>>, Arc<Vec<Arc<Box<GameObject>>>>)>>, receiver: &Receiver<Arc<Box<GameObject>>>) -> Arc<Vec<Arc<Box<GameObject>>>> {
            for x in 0..states.len() {
                let states = states.clone();
                let state = states[x].clone();
                sends[x % sends.len()].send((state, states)).unwrap();
            }
            let mut next_states: Vec<Arc<Box<GameObject>>> = vec![];
            for _ in 0..states.len() {
                let state = receiver.recv().unwrap();
                match *socket {
                    Some(ref s) => {
                        match port.clone() {
                            Some(p) => {
                                let json = &mut serde_json::ser::Serializer::new(io::stdout());
                                let mut formats: Map<&str, Box<erased_serde::Serializer>> = Map::new();
                                formats.insert("json", Box::new(erased_serde::Serializer::erase(json)));
                                let mut values: Map<&str, Box<GameObject>> = Map::new();
                                values.insert("state", state.deref().box_clone());
                                let format = formats.get_mut("json").unwrap();
                                let value = values.get("state").unwrap();
                                value.erased_serialize(format).unwrap();
                                // let serialized = state.serialize().unwrap();
                                // let serialized = serde_json::to_string(&state).unwrap();
                                let buf = [0;10];
                                let addr: String = format!("127.0.0.1:{}", p);
                                match s.send_to(&buf, addr) {
                                    Ok(_) => {},
                                    Err(e) => println!("Failed to send: {}", e)
                                }
                            },
                            None => {}
                        }
                    },
                    None => {}
                }
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
    use std::str::FromStr;
    use fungine::{ Fungine, GameObject, Message };
    use stopwatch::{Stopwatch};

    #[derive(Clone, Serialize, Deserialize, Debug)]
    struct TestGameObject {
        value: i32,
    }

    impl GameObject for TestGameObject {
        fn box_clone(&self) -> Box<GameObject> {
            Box::new((*self).clone())
        }

        fn update(&self, current_state: Arc<Vec<Arc<Box<GameObject>>>>, messages: Vec<Message>) -> Box<GameObject> {
            Box::new(TestGameObject {
                value: self.value + 1
            })
        }
    }
    unsafe impl Send for TestGameObject {}
    unsafe impl Sync for TestGameObject {}


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
            assert!(next_object.value == 1);
        }
        else {
            assert!(false);
        }
    }

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
                assert!(object.value == 1000);
            }
            else {
                assert!(false);
            }
        }
    }

    #[test]
    fn network_test() {
        let initial_object = TestGameObject {
            value: 0
        };
        let port = "4794";
        let soc = UdpSocket::bind(format!("127.0.0.1:{}", port));
        let socket = match soc {
            Ok(s) => Some(s),
            Err(_) => panic!("Couldn't open listen socket")
        };
        let initial_object = Box::new(initial_object) as Box<GameObject>;
        let initial_object = Arc::new(initial_object);
        let initial_state = Arc::new(vec![initial_object]);
        let engine = Fungine::new(initial_state, Some(String::from_str(port)).unwrap().ok());
        let _ = engine.run_steps(1);
        let mut buf = [0; 1024];
        let (amt, src) = socket.recv_from(&mut buf)?;

    }
}