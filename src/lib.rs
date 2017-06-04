#[macro_use]
extern crate downcast_rs;
extern crate stopwatch;
extern crate futures;
extern crate futures_cpupool;

pub mod fungine {
	use std::sync::Arc;
	use downcast_rs::Downcast;
	use futures::Future;
	use futures_cpupool::{CpuPool, CpuFuture};

	pub struct Message {

	}

	pub trait GameObject: Downcast {
		fn update(&self, current_state: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>>, messages: Vec<Message>) -> Box<GameObject+Send+Sync>;
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
		initial_state: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>>,
		pool: CpuPool
	}

	impl Fungine {
		pub fn new(initial_state: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>>) -> Fungine {
			let pool = CpuPool::new_num_cpus();

			Fungine {
				initial_state: initial_state,
				pool: pool
			}
		}

		pub fn run(self) {
			let mut states: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>> = self.initial_state;

			loop {
				states = Fungine::step_engine(states, &self.pool);
			}
		}

		pub fn run_steps(self, steps: u32) -> Arc<Vec<Arc<Box<GameObject+Send+Sync>>>> {
			let mut states: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>> = self.initial_state;

			for _ in 0..steps {
				states = Fungine::step_engine(states, &self.pool);
			}

			states
		}

		fn step_engine(states: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>>, pool: &CpuPool) -> Arc<Vec<Arc<Box<GameObject+Send+Sync>>>> {
			let mut next_states: Vec<Arc<Box<GameObject+Send+Sync>>> = vec![];
			let mut futures = vec![];
			for x in 0..states.len() {
				let states = states.clone();
				let state = states[x].clone();
				let future: CpuFuture<Box<GameObject+Send+Sync>, String> = pool.spawn_fn(move || {
					let messages: Vec<Message> = Vec::new();
					Ok(state.update(states, messages))
				});
				futures.push(future);
			}
			for future in futures {
				match future.wait() {
					Ok(s) => next_states.push(Arc::new(s)),
					Err(_) => panic!("Future failed to return")
				}
			}
			Arc::new(next_states)

//			for x in 0..states.len() {
//				let states = states.clone();
//				let state = states[x].clone();
//				sends[x % sends.len()].send((state, states)).unwrap();
//			}
//			let mut next_states: Vec<Arc<Box<GameObject+Send+Sync>>> = vec![];
//			for _ in 0..states.len() {
//				next_states.push(receiver.recv().unwrap());
//			}
//			Arc::new(next_states)
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

	    fn update(&self, current_state: Arc<Vec<Arc<Box<GameObject+Send+Sync>>>>, messages: Vec<Message>) -> Box<GameObject+Send+Sync> {
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
    	let initial_object = Box::new(initial_object) as Box<GameObject+Send+Sync>;
		let initial_object = Arc::new(initial_object);
    	let initial_state = Arc::new(vec![initial_object]);
    	let engine = Fungine::new(initial_state);
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
    		let initial_object = Box::new(initial_object) as Box<GameObject+Send+Sync>;
			let initial_object = Arc::new(initial_object);
    		initial_state.push(initial_object);
    	}
		let engine = Fungine::new(Arc::new(initial_state));
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
}