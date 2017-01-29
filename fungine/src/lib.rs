#[macro_use]
extern crate downcast_rs;

mod fungine {
	use std::sync::Arc;
	use downcast_rs::Downcast;

	pub struct Message {

	}

	pub trait GameObject: Downcast {
		fn update(self: Box<Self>, current_state: &Vec<Box<GameObject>>, messages: Vec<Message>) -> Box<GameObject>;
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
		initial_state: Vec<Box<GameObject>>,
	}

	impl Fungine {
		pub fn run(self) {
			let mut states: Arc<Vec<Box<GameObject>>> = Arc::new(self.initial_state);
			loop {
				states = Fungine::step_engine(states);
			}
		}

		pub fn step_engine(states: Arc<Vec<Box<GameObject>>>) -> Arc<Vec<Box<GameObject>>> {
			let mut next_states: Vec<Box<GameObject>> = Vec::new();
			for x in 0..states.len() {
				let states = states.clone();
				let messages: Vec<Message> = Vec::new();
				let state = states[x].clone();
				let next_state = state.update(&states, messages);
				next_states.push(next_state);
			}
			Arc::new(next_states)
		}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use fungine::{ Fungine, GameObject, Message };

	#[derive(Clone)]
	struct TestGameObject {
		value: i32,
	}

	impl GameObject for TestGameObject {
	    fn box_clone(&self) -> Box<GameObject> {
	        Box::new((*self).clone())
	    }

	    fn update(self: Box<Self>, current_state: &Vec<Box<GameObject>>, messages: Vec<Message>) -> Box<GameObject> {
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
    	let initial_state = Arc::new(vec![initial_object]);
    	let next_state = Fungine::step_engine(initial_state);
    	if let Some(next_object) = next_state[0].downcast_ref::<TestGameObject>() {
    		assert!(next_object.value == 1);
    	}
    	else {
    		assert!(false);
    	}
    }
}