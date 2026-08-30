use rustdb::parser::{parse};

#[test]
fn lexer_test(){
    let query = String::from("{\"table\":\"users\",\"task\":\"select\",\"conditions\":{\"column\":\"age\",\"operator\":\"ge\",\"value\":\"25\"},\"columns\":{\"column\":\"age\",\"column\":\"name\"}}");
    parse(query);
}




