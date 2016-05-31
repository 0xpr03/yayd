/*use super::{Registry,Module,HandleData};
use lib::{Error,Request};

/// Init youtube handler, registering it
pub fn init(registry: &mut Registry){
    
    registry.register(Module {
    	checker: Box::new(checker),
    	handler: Box::new(handler),
    	extended_cleanup: false,
    });
}

/// Check if the data matches for this handler
fn checker(data: & Request) -> bool {
    false
}

/// Process data
fn handler(handle_db: &mut HandleData, request: & Request) -> Result<(),Error> {
    Ok(())
}*/