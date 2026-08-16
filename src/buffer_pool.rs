use std::collections::HashMap;
use crate::slotted_page::{Page};
use std::vec::Vec;
use std::fmt;
use crate::page::CorruptedDataError;

struct DLLNode{
    prev : Option<usize>,
    next : Option<usize>,
    page : Page,
    idx  : usize,
}

struct DLL{
    head        :   Option<usize>,
    tail        :   Option<usize>,
    nodes       :   Vec<DLLNode>,
    trash       :   Vec<usize>,
    capacity    :   usize,
}

impl DLL{
    fn new(capacity:usize)->Self{
        let nodes:Vec<DLLNode>=Vec::new();
        let trash:Vec<usize>=Vec::new();
        Self{
            head:None,
            tail:None,
            nodes,
            trash,
            capacity,
        }
    }
    fn add_node(&mut self,page : Page)->Result<(),CorruptedDataError>{
        if !self.trash.is_empty(){
            let idx=match self.trash.pop(){
                Some(i)=>i,
                None=>return Err(CorruptedDataError),
            };
            let new_node=DLLNode::new(page,idx);
            self.nodes[idx]=new_node;
            match self.head{
                Some(old_head)=>{
                    self.nodes[old_head].prev=Some(idx);
                    self.nodes[idx].next=Some(old_head);
                    self.head=Some(idx);
                }
                None=>{
                    self.head=Some(idx);
                    self.tail=Some(idx);
                }
            };
        }else{
            let idx:usize=self.nodes.len();
            let new_node=DLLNode::new(page,idx);
            self.nodes.push(new_node);
            match self.head{
                Some(old_head)=>{
                    self.nodes[old_head].prev=Some(idx);
                    self.nodes[idx].next=Some(old_head);
                    self.head=Some(idx);
                }
                None=>{
                    self.head=Some(idx);
                    self.tail=Some(idx);
                }
            }
        }
        self.shrink();
        Ok(())
    }
    fn shrink(&mut self){
        while self.nodes.len()-self.trash.len()>self.capacity{
            self.pop_tail();
        }
    }
    fn move_to_head(&mut self,idx:usize)->Result<(),CorruptedDataError>{
        let head=match self.head{
            Some(h)=>h,
            None=>return Err(CorruptedDataError),
        };
        let tail=match self.tail{
            Some(t)=>t,
            None=>return Err(CorruptedDataError),
        };
        if head==tail && idx==head{
            return Ok(());
        }
        else if idx==head{
            return Ok(());
        }
        let prev_idx=match self.nodes[idx].prev{
            Some(p)=>p,
            None=>return Err(CorruptedDataError),
        };
        if idx==tail{
            self.nodes[prev_idx].next=None;
            self.tail=Some(prev_idx);
        }
        else{
            let next_idx=match self.nodes[idx].next{
                Some(n)=>n,
                None=>return Err(CorruptedDataError),
            };
            self.nodes[prev_idx].next=Some(next_idx);
            self.nodes[next_idx].prev=Some(prev_idx);
        }
        self.nodes[idx].prev=None;
        self.nodes[idx].next=Some(head);
        self.head=Some(idx);
        Ok(())
    }
    fn delete_node(&mut self,idx:usize)->Result<(),CorruptedDataError>{
        let head=match self.head{
            Some(h)=>h,
            None=>return Err(CorruptedDataError),
        };
        let tail=match self.tail{
            Some(t)=>t,
            None=>return Err(CorruptedDataError),
        };
        if idx==head{
            if idx==tail{
                self.head=None;
                self.tail=None;
                self.trash.push(idx);
            }else{
                let next_idx=match self.nodes[idx].next{
                    Some(n)=>n,
                    None=>return Err(CorruptedDataError),
                };
                self.nodes[next_idx].prev=None;
                self.head=Some(next_idx);
                self.trash.push(idx);
            }
        }
        else if idx==tail{
            let prev_idx=match self.nodes[idx].prev{
                Some(p)=>p,
                None=>return Err(CorruptedDataError),
            };
            self.nodes[prev_idx].next=None;
            self.tail=Some(prev_idx);
            self.trash.push(idx);
        }
        else{
            let next_idx=match self.nodes[idx].next{
                Some(n)=>n,
                None=>return Err(CorruptedDataError),
            };
            let prev_idx=match self.nodes[idx].prev{
                Some(p)=>p,
                None=>return Err(CorruptedDataError),
            };
            self.nodes[prev_idx].next=Some(next_idx);
            self.nodes[next_idx].prev=Some(prev_idx);
            self.trash.push(idx);
        }
        Ok(())
    }
    fn pop_tail(&mut self)->Result<(),CorruptedDataError>{
        let sentinel:usize=self.capacity+1;
        let head=match self.head{
            Some(h)=>h,
            None=>sentinel,
        };
        let tail=match self.tail{
            Some(t)=>t,
            None=>sentinel,
        };
        if tail==sentinel && head==sentinel{
            return Ok(());
        }
        else if (tail==sentinel && head!=sentinel)||(tail!=sentinel && head==sentinel){
            return Err(CorruptedDataError);
        }
        else if tail==head{
            self.tail=None;
            self.head=None;
            self.trash.push(tail);
        }
        else{
            let prev_idx=match self.nodes[tail].prev{
                Some(p)=>p,
                None=>return Err(CorruptedDataError),
            };
            self.nodes[prev_idx].next=None;
            self.tail=Some(prev_idx);
            self.trash.push(tail);
        }
        Ok(())
    }
}

impl DLLNode{
    fn new(page : Page,idx:usize)->Self{
        Self{
            prev : None,
            next : None,
            page,
            idx,
        }
    }
}
