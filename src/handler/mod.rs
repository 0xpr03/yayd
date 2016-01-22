
use monster::incubation::FindAndTake;

mod youtube;
mod soundcloud;
mod twitch;

use std::vec::Vec;

macro_rules! regex(
    ($s:expr) => (regex::Regex::new($s).unwrap());
);

pub struct Module {
    checker: Box<Fn(i32) -> bool>,
    action: Box<Fn(&mut Registry, i32) -> i32>
}

pub struct Registry {
    modules: Vec<Module>
}

impl Registry {
	pub fn new() -> Registry{
        Registry {modules: Vec::new()}
    }
	
    fn register(&mut self, module: Module) {
        self.modules.push(module);
    }
    
    fn handle_some_data(&mut self, data: i32) {
	    let module = self.modules.find_and_take(|module| (module.checker)(data));
	    
        if let Some(module) = module {
	       (module.action)(self, data);
	       self.modules.push(module);
        }
	}
}

pub fn init_handlers() -> Registry {
	let mut registry = Registry::new();
	
	
	registry
}