use linked_list::LinkedList;
use linked_list::ComputeNorm;
pub mod linked_list;

fn main() {
    let mut list: LinkedList<u32> = LinkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.get_size(), 0);
    for i in 1..12 {
        list.push_front(i);
    }
    println!("{}", list);
    println!("list size: {}", list.get_size());
    println!("top element: {}", list.pop_front().unwrap());
    println!("{}", list);
    println!("size: {}", list.get_size());
    println!("{}", list.to_string()); // ToString impl for anything impl Display

    let mut str_list: LinkedList<String> = LinkedList::new();
    assert!(str_list.is_empty());
    assert_eq!(str_list.get_size(), 0);
    str_list.push_front("abc".to_string());
    str_list.push_front("eric".to_string());
    str_list.push_front("jessie".to_string());

    println!("{}", str_list);
    println!("list size: {}", str_list.get_size());
    println!("top element: {}", str_list.pop_front().unwrap());
    println!("{}", str_list);
    println!("size: {}", str_list.get_size());
    println!("{}", str_list.to_string()); // ToString impl for anything impl Display

    let mut new_list = list.clone();
    assert_eq!(new_list, list);
    new_list.pop_front();
    println!("{}", new_list);
    println!("{}", list);
    assert_ne!(new_list, list);

    // If you implement iterator trait:
    for v in &new_list {
        println!("{}", v);
    }

    let mut float_list: LinkedList<f64> = LinkedList::new();
    for i in 1..12 {
        float_list.push_front(i as f64);
    }
    println!("norm: {}", float_list.compute_norm());
}
