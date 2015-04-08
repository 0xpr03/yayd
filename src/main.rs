static VERSION : &'static str = "0.1"; // String not valid


fn main() {
    println!("Starting yayd-backend v{}",&VERSION);


}

/// Set options for the connection
fn mysql_options() -> MyOpts {
    MyOpts {
    	tcp_addr: Some("127.0.0.1".to_string()),
    	tcp_port: 3306,
    	user: Some("root".to_string()),
    	pass: Some("".to_string()),
    	db_name: Some("testdb".to_string()),
    	..Default::default() // set other to default
    }
}