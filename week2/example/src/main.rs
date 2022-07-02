// fn main() {
//     let mut s = String::from("hello");
//     let ref1 = &s.clone();
//     let ref2 = &ref1;
//     let ref3 = &ref2;
//     s = String::from("goodbye");
//     println!("{}", ref3.to_uppercase());
// }

fn main() {
    let s1 = String::from("hello");
    let mut v = Vec::new();
    v.push(s1);
    let s2: &String = &v[0];
    println!("{}", s2);
}

fn drip_drop() -> Box<String> {
    let s = String::from("hello world!");
    return Box::new(s);
}
