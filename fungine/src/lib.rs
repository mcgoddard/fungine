#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

mod fungine {
	use std::sync::Arc;

	struct Message {

	}

	trait GameObject {
		fn update(self: Box<Self>, current_state: &Vec<Box<GameObject>>, messages: Vec<Message>) -> Box<GameObject>;
		fn box_clone(&self) -> Box<GameObject>;
	}

	impl Clone for Box<GameObject>
	{
	    fn clone(&self) -> Box<GameObject> {
	        self.box_clone()
	    }
	}

	struct Fungine {
		initial_state: Vec<Box<GameObject>>,
	}

	impl Fungine {
		fn run(self) {
			let mut next_states: Vec<Box<GameObject>> = Vec::new();
			let states: Arc<Vec<Box<GameObject>>> = Arc::new(Vec::new());
			for x in 0..states.len() {
				let states = states.clone();
				let messages: Vec<Message> = Vec::new();
				let state = states[x].clone();
				let next_state = state.update(&states, messages);
				next_states.push(next_state);
			}
		}
	}
}