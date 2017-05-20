#[macro_use]
extern crate downcast_rs;
extern crate stopwatch;

mod fungine {
	use std::sync::Arc;
	use downcast_rs::Downcast;

	pub struct Message {

	}

	pub trait GameObject: Downcast {
		fn update(&self, current_state: Arc<Vec<Arc<Box<GameObject>>>>, messages: Vec<Message>) -> Box<GameObject>;
		fn box_clone(&self) -> Box<GameObject>;
	}
	impl_downcast!(GameObject);

	impl Clone for Box<GameObject>
	{
	    fn clone(&self) -> Box<GameObject> {
	        self.box_clone()
	    }
	}

	pub struct Fungine {
		initial_state: Arc<Vec<Arc<Box<GameObject>>>>,
	}

	impl Fungine {
		pub fn new(initial_state: Arc<Vec<Arc<Box<GameObject>>>>) -> Fungine {
			Fungine {
				initial_state: initial_state
			}
		}

		pub fn run(self) {
			let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state;
			loop {
				states = Fungine::step_engine(states);
			}
		}

		pub fn run_steps(self, steps: u32) -> Arc<Vec<Arc<Box<GameObject>>>> {
			let mut states: Arc<Vec<Arc<Box<GameObject>>>> = self.initial_state;
			for _ in 0..steps {
				states = Fungine::step_engine(states);
			}
			states
		}

		fn step_engine(states: Arc<Vec<Arc<Box<GameObject>>>>) -> Arc<Vec<Arc<Box<GameObject>>>> {
			let mut next_states: Vec<Arc<Box<GameObject>>> = Vec::new();
			for x in 0..states.len() {
				let states = states.clone();
				let state = states[x].clone();
				let messages: Vec<Message> = Vec::new();
				let next_state = state.update(states, messages);
				next_states.push(Arc::new(next_state));
			}
			Arc::new(next_states)
		}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use fungine::{ Fungine, GameObject, Message };
	use stopwatch::{Stopwatch};

	#[derive(Clone)]
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

    #[test]
    fn test_iterate() {
    	let initial_object = TestGameObject {
    		value: 0
    	};
    	let initial_object = Box::new(initial_object) as Box<GameObject>;
    	let initial_state = Arc::new(vec![Arc::new(initial_object)]);
		let engine = Fungine::new(initial_state);
    	let next_state = engine.run_steps(1);
    	if let Some(next_object) = next_state[0].downcast_ref::<TestGameObject>() {
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
    		initial_state.push(Arc::new(initial_object));
    	}
		let initial_state = Arc::new(initial_state);
    	let sw = Stopwatch::start_new();
		let engine = Fungine::new(initial_state);
    	let final_states = engine.run_steps(1000);
    	for x in 0..final_states.len() {
			let state = final_states[x].clone();
    		if let Some(state) = state.downcast_ref::<TestGameObject>() {
    			assert!(state.value == 1000);
	    	}
	    	else {
	    		assert!(false);
	    	}
    	}
    	println!("Time taken: {}ms", sw.elapsed_ms());
    }
}