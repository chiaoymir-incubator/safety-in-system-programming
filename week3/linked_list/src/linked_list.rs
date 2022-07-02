use std::fmt;
use std::option::Option;

#[derive(Debug)]
pub struct LinkedList<T> {
    head: Option<Box<Node<T>>>,
    size: usize,
}

#[derive(Debug)]
struct Node<T> {
    value: T,
    next: Option<Box<Node<T>>>,
}

impl<T> Node<T> {
    pub fn new(value: T, next: Option<Box<Node<T>>>) -> Node<T> {
        Node {value: value, next: next}
    }
}

impl<T> LinkedList<T> {
    pub fn new() -> LinkedList<T> {
        LinkedList {head: None, size: 0}
    }
    
    pub fn get_size(&self) -> usize {
        self.size
    }
    
    pub fn is_empty(&self) -> bool {
        self.get_size() == 0
    }
    
    pub fn push_front(&mut self, value: T) {
        let new_node: Box<Node<T>> = Box::new(Node::new(value, self.head.take()));
        self.head = Some(new_node);
        self.size += 1;
    }
    
    pub fn pop_front(&mut self) -> Option<T> {
        let node: Box<Node<T>> = self.head.take()?;
        self.head = node.next;
        self.size -= 1;
        Some(node.value)
    }
}


impl<T: fmt::Display> fmt::Display for LinkedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut current: &Option<Box<Node<T>>> = &self.head;
        let mut result = String::new();
        loop {
            match current {
                Some(node) => {
                    result = format!("{} {}", result, node.value);
                    current = &node.next;
                },
                None => break,
            }
        }
        write!(f, "{}", result)
    }
}

impl<T: Clone> Clone for LinkedList<T> {
    fn clone(&self) -> Self {
        let mut new_list = LinkedList::new();
        let mut nodes = vec![];
        let mut current: &Option<Box<Node<T>>> = &self.head;
        loop {
            match current {
                Some(node) => {
                    nodes.push(node.value.clone());
                    current = &node.next;
                },
                None => break,
            }
        }

        let l = nodes.len();
        let mut i = 0;
        while i < l {
            new_list.push_front(nodes[l - 1 - i].clone());
            i += 1;
        }

        new_list
    }
}

impl<T: PartialEq> PartialEq for LinkedList<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.size != other.size {
            return false
        }

        let mut self_current: &Option<Box<Node<T>>> = &self.head;
        let mut other_current: &Option<Box<Node<T>>> = &other.head;

        loop {
            match (self_current, other_current) {
                (Some(self_node), Some(other_node))  => {
                    if self_node.value != other_node.value {
                        return false
                    }
                    self_current = &self_node.next;
                    other_current = &other_node.next;
                },
                (None, None) => break,
                _ => return false,
            }
        }

        true
    }
}

pub trait ComputeNorm {
    fn compute_norm(&self) -> f64 {
        0.0
    }
}

impl ComputeNorm for LinkedList<f64> {
    fn compute_norm(&self) -> f64 {
        self.into_iter().map(|x| {x * x}).sum::<f64>().sqrt()
    }
}

pub struct LinkedListIter<'a, T> {
    current: &'a Option<Box<Node<T>>>,
}


impl<T: Clone> Iterator for LinkedListIter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        match self.current {
            Some(node) => {
                self.current = &node.next;
                Some(node.value.clone())
            },
            None => None,
        }
    }
}

impl<'a, T: Clone> IntoIterator for &'a LinkedList<T> {
    type Item = T;
    type IntoIter = LinkedListIter<'a, T>;
    fn into_iter(self) -> LinkedListIter<'a, T> {
        LinkedListIter {current: &self.head}
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.head.take();
        while let Some(mut node) = current {
            current = node.next.take();
        }
    }
}



