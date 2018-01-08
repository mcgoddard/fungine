#[macro_use]
extern crate downcast_rs;
extern crate stopwatch;
extern crate num_cpus;

pub mod fungine {
    use std::thread;
    use std::sync::Arc;
    use std::sync::mpsc::Sender;
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc;
    use std::collections::hash_map::{HashMap, Entry};
    use downcast_rs::Downcast;
    use num_cpus;
    use stopwatch::{Stopwatch};

    // A trait representing a message between GameObjects.
    pub trait Message: Downcast + Send + Sync {
        fn box_clone(&self) -> Box<Message>;
    }
    impl_downcast!(Message);

    impl Clone for Box<Message>
    {
        fn clone(&self) -> Box<Message> {
            self.box_clone()
        }
    }

    // The GameObject trait will be implemented by everything that wants to be executed
    // as part of a frame step by the engine.
    pub trait GameObject: Downcast + Send + Sync {
        fn update(&self, id: u64, current_state: Arc<Vec<GameObjectWithID>>, messages: Arc<Vec<Arc<Box<Message>>>>, time: f32) -> UpdateResult;
        fn box_clone(&self) -> Box<GameObject>;
    }
    impl_downcast!(GameObject);

    impl Clone for Box<GameObject>
    {
        fn clone(&self) -> Box<GameObject> {
            self.box_clone()
        }
    }

    #[derive(Clone)]
    pub struct GameObjectWithState(GameObjectWithID, Arc<Vec<GameObjectWithID>>, f32, Arc<Vec<Arc<Box<Message>>>>);

    #[derive(Clone)]
    pub struct GameObjectWithID {
        pub id: u64,
        pub game_object: Arc<Box<GameObject>>
    }

    #[derive(Clone)]
    pub struct MessageWithID {
        pub id: u64,
        pub message: Arc<Box<Message>>
    }

    #[derive(Clone)]
    pub struct UpdateResult {
        pub state: Box<GameObject>,
        pub messages: Vec<Arc<Box<MessageWithID>>>
    }

    #[derive(Clone)]
    pub struct UpdateResultWithID {
        pub id: u64,
        pub result: UpdateResult
    }

    // The main engine structure. This stores the state, communications and networking objects.
    pub struct Fungine {
        pub initial_state: Arc<Vec<GameObjectWithID>>,
        sends: Vec<Sender<GameObjectWithState>>,
        receiver: Receiver<UpdateResultWithID>,
        pub current_state: Arc<Vec<GameObjectWithID>>,
        pub messages: Arc<HashMap<u64, Arc<Vec<Arc<Box<Message>>>>>>
    }

    impl Fungine {
        // Constructor that sets up the engine with an initial state, worker threads and networking.
        pub fn new(initial_state: &Arc<Vec<GameObjectWithID>>) -> Fungine {
            // Create channels for sending objects to process to worker threads.
            let (send_modified, receive_modified): (Sender<UpdateResultWithID>, Receiver<UpdateResultWithID>) = mpsc::channel();
            let receiver = receive_modified;
            let mut sends = vec![];
            // Create worker threads
            let mut thread_count = num_cpus::get();
            if thread_count <= 2 {
                thread_count = 1;
            }
            else {
                thread_count -= 2;
            }
            for _ in 0..thread_count {
                let send_modified = send_modified.clone();
                let (send_original, receive_original) = mpsc::channel();
                sends.push(send_original);

                thread::spawn(move || {
                    // A worker thread will pull GameObjects, execute their update method and send the new
                    // state back to the main thread.
                    loop {
                        match receive_original.recv() {
                            Ok(original) => {
                                let original: GameObjectWithState = original;
                                let current_object: Arc<Box<GameObject>> = Arc::clone(&(original.0).game_object);
                                let current_state: Arc<Vec<GameObjectWithID>> = Arc::clone(&original.1);
                                let messages: Arc<Vec<Arc<Box<Message>>>> = original.3;
                                let new = current_object.update((original.0).id, current_state, messages, original.2);
                                send_modified.send(UpdateResultWithID {
                                    id: (original.0).id,
                                    result: new
                                }).unwrap();
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
                initial_state: Arc::clone(initial_state),
                sends: sends,
                receiver: receiver,
                current_state: Arc::clone(initial_state),
                messages: Arc::new(HashMap::new())
            }
        }

        // Step the engine forward indefinitely.
        pub fn run(&self) {
            let mut states: Arc<Vec<GameObjectWithID>> = Arc::clone(&self.initial_state);
            let mut sw = Stopwatch::start_new();
            let mut frame_count = 0;
            let mut messages = Arc::clone(&self.messages);

            loop {
                if sw.elapsed_ms() > 10_000 {
                    println!("Frames processed per second: {}", frame_count / 10);
                    sw.restart();
                    frame_count = 0;
                }
                let result = Fungine::step_engine(&states, &self.sends, &self.receiver, (sw.elapsed_ms()/1000) as f32, &messages);
                states = result.0;
                messages = result.1;
                frame_count += 1;
                sw.restart();
            }
        }

        // Step the engine forward a specified number of steps, used for testing.
        pub fn run_steps(&self, steps: u32, time_between: f32) -> Arc<Vec<GameObjectWithID>> {
            let mut states: Arc<Vec<GameObjectWithID>> = Arc::clone(&self.initial_state);
            let mut messages = Arc::clone(&self.messages);

            for _ in 0..steps {
                let result = Fungine::step_engine(&states, &self.sends, &self.receiver, time_between, &messages);
                states = Arc::clone(&result.0);
                messages = Arc::clone(&result.1);
            }

            states
        }

        // Step the engine forward a specified number of steps from a provided state.
        pub fn run_steps_cont(&mut self, steps: u32, time_between: f32) -> Arc<Vec<GameObjectWithID>> {
            let mut states: Arc<Vec<GameObjectWithID>> = Arc::clone(&self.current_state);
            let mut messages = Arc::clone(&self.messages);

            for _ in 0..steps {
                let result = Fungine::step_engine(&states, &self.sends, &self.receiver, time_between, &messages);
                states = Arc::clone(&result.0);
                messages = result.1;
            }

            self.current_state = Arc::clone(&states);
            self.messages = messages;

            states
        }

        // Perform one step by processing each GameObject in the state once.
        fn step_engine(states: &Arc<Vec<GameObjectWithID>>, sends: &[Sender<GameObjectWithState>], 
            receiver: &Receiver<UpdateResultWithID>, time: f32, messages: &Arc<HashMap<u64, Arc<Vec<Arc<Box<Message>>>>>>)
            -> (Arc<Vec<GameObjectWithID>>, Arc<HashMap<u64, Arc<Vec<Arc<Box<Message>>>>>>) {
            // Send current states to the worker threads
            for i in 0..states.len() {
                let states = Arc::clone(states);
                let state = states[i].clone();
                let message: Arc<Vec<Arc<Box<Message>>>>;
                if let Some(m) = messages.get(&state.id) {
                    message = m.clone();
                }
                else {
                    message = Arc::new(vec![]);
                }
                sends[i % sends.len()].send(GameObjectWithState(state, states, time, message)).unwrap();
            }
            let mut next_states: Vec<GameObjectWithID> = vec![];
            let mut next_messages: HashMap<u64, Arc<Vec<Arc<Box<Message>>>>> = HashMap::new();
            // Collect new states
            for _ in 0..states.len() {
                let result = receiver.recv().unwrap();
                next_states.push(GameObjectWithID {
                    id: result.id,
                    game_object: Arc::new(result.result.state)
                });
                let messages = result.result.messages;
                for message in &messages {
                    let message = message.clone();
                    match next_messages.entry(result.id) {
                        Entry::Occupied(mut entry) => {
                            let entry = entry.get_mut();
                            if let Some(m) = Arc::get_mut(entry) {
                                m.push(message.message.clone());
                            }
                        },
                        Entry::Vacant(entry) => {
                            entry.insert(Arc::new(vec![message.message.clone()]));
                        }
                    }
                }
            }
            (Arc::new(next_states), Arc::new(next_messages))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use fungine::{ Fungine, GameObject, Message, GameObjectWithID, UpdateResult, MessageWithID };
    use stopwatch::{ Stopwatch };

    // A GameObject implementation with some state
    #[derive(Clone, Debug)]
    struct TestGameObject {
        pub value: i32,
    }

    impl GameObject for TestGameObject {
        fn box_clone(&self) -> Box<GameObject> {
            Box::new((*self).clone())
        }

        // The update function simply increments the internal counter
        fn update(&self, _id: u64, _current_state: Arc<Vec<GameObjectWithID>>, _messages: Arc<Vec<Arc<Box<Message>>>>, _frame_time: f32) -> UpdateResult {
            UpdateResult {
                state: Box::new(TestGameObject {
                    value: self.value + 1
                }),
                messages: vec![]
            }
        }
    }
    unsafe impl Send for TestGameObject {}
    unsafe impl Sync for TestGameObject {}

    #[derive(Clone, Debug)]
    struct TestMessage {
        pub value: i32
    }

    impl Message for TestMessage {
        fn box_clone(&self) -> Box<Message> {
            Box::new((*self).clone())
        }
    }
    unsafe impl Send for TestMessage {}
    unsafe impl Sync for TestMessage {}


    #[derive(Clone, Debug)]
    struct MessageGameObject {
        pub value: i32
    }

    impl GameObject for MessageGameObject {
        fn box_clone(&self) -> Box<GameObject> {
            Box::new((*self).clone())
        }

        // The update function simply increments the internal counter
        fn update(&self, id: u64, current_state: Arc<Vec<GameObjectWithID>>, messages: Arc<Vec<Arc<Box<Message>>>>, _frame_time: f32) -> UpdateResult {
            let mut new_value: i32 = self.value;
            for message in messages.clone().iter() {
                let message: Box<Message> = message.box_clone();
                if let Some(message) = message.downcast_ref::<TestMessage>() {
                    new_value += message.value;
                }
            }
            let mut new_messages = vec![];
            for state in current_state.iter() {
                if state.id != id {
                    new_messages.push(Arc::new(Box::new(MessageWithID {
                        id: state.id,
                        message: Arc::new(Box::new(TestMessage {
                            value: 1
                        }))
                    })));
                }
            }
            UpdateResult {
                state: Box::new(MessageGameObject {
                    value: new_value
                }),
                messages: new_messages
            }
        }
    }
    unsafe impl Send for MessageGameObject {}
    unsafe impl Sync for MessageGameObject {}


    // A single object, single iteration test to make sure state is passed around correctly
    #[test]
    fn test_iterate() {
        let initial_object = TestGameObject {
            value: 0
        };
        let initial_object = Box::new(initial_object) as Box<GameObject>;
        let initial_object = Arc::new(initial_object);
        let initial_object = GameObjectWithID {
            id: 0u64, 
            game_object: initial_object
        };
        let initial_state = Arc::new(vec![initial_object]);
        let engine = Fungine::new(&initial_state);
        let next_states = engine.run_steps(1, 1f32);
        let next_state = next_states[0].clone();
        let next_state: Box<GameObject> = (next_state.game_object).box_clone();
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
        for i in 0..1000 {
            let initial_object = TestGameObject {
                value: 0
            };
            let initial_object = Box::new(initial_object) as Box<GameObject>;
            let initial_object = Arc::new(initial_object);
            let initial_object = GameObjectWithID {
                id: i, 
                game_object: initial_object
            };
            initial_state.push(initial_object);
        }
        let engine = Fungine::new(&Arc::new(initial_state));
        let sw = Stopwatch::start_new();
        let final_states = engine.run_steps(1000, 1f32);
        println!("Time taken: {}ms", sw.elapsed_ms());
        for x in 0..final_states.len() {
            let final_state = final_states[x].clone();
            let final_state: Box<GameObject> = (final_state.game_object).box_clone();
            if let Some(object) = final_state.downcast_ref::<TestGameObject>() {
                assert_eq!(1000, object.value);
            }
            else {
                assert!(false);
            }
        }
    }

    // Test messages can be sent, read and processed
    #[test]
    fn message_test() {
        let mut initial_state = vec![];
        for i in 0u64..3u64 {
            let initial_object = MessageGameObject {
                value: 0
            };
            let initial_object = Box::new(initial_object) as Box<GameObject>;
            let initial_object = Arc::new(initial_object);
            let initial_object = GameObjectWithID {
                id: i,
                game_object: initial_object
            };
            initial_state.push(initial_object);
        }
        let initial_state = Arc::new(initial_state);
        let engine = Fungine::new(&initial_state);
        let next_states = engine.run_steps(2, 1f32);
        for i in 0..3 {
            let next_state = next_states[i].clone();
            let next_state: Box<GameObject> = (next_state.game_object).box_clone();
            if let Some(next_object) = next_state.downcast_ref::<MessageGameObject>() {
                assert_eq!(2, next_object.value);
            }
            else {
                assert!(false);
            }
        }
    }
}