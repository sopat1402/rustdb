use std::collections::HashMap;
use crate::slotted_page::{Page,RecordError};
use std::vec::Vec;
use crate::page::{DatabaseFile};
use crate::db_errors::CorruptedDataError;

pub struct DLLNode{
    prev : Option<usize>,
    next : Option<usize>,
    pub page : Page,
    idx  : usize,
}

pub struct LRUCache{
    pub map     :   HashMap<u64,usize>, //page id : node index
    pub dll     :   DLL,
}

pub struct DLL{
    pub head        :   Option<usize>,
    tail        :   Option<usize>,
    pub nodes       :   Vec<DLLNode>,
    pub trash       :   Vec<usize>,
    capacity    :   usize,
}

impl LRUCache{
    pub fn new(capacity:usize)->Self{
        let dll=DLL::new(capacity);
        let map:HashMap<u64,usize>=HashMap::new();
        Self{
            map,
            dll,
        }
    }
    pub fn get_index(&mut self,page_id:u64)->Result<usize,RecordError>{
        match self.map.get(&page_id){
            Some(&idx)=>{
                match self.dll.move_to_head(idx){
                    Ok(_)=>Ok(idx),
                    Err(_)=>Err(RecordError::RecordMismatch),
                }
            }
            None=>Err(RecordError::RecordAbsent),
        }
    }
    pub fn get(&mut self,page_id:u64)->Result<&DLLNode,RecordError>{
        match self.map.get(&page_id){
            Some(idx)=>{
                match self.dll.move_to_head(*idx){
                    Ok(_)=>{},
                    Err(_)=>return Err(RecordError::RecordMismatch),
                };
                let node : &DLLNode=&self.dll.nodes[*idx];
                return Ok(node)
            }
            None=>{
                return Err(RecordError::RecordAbsent)
            }
        };
    }
    pub fn get_mut(&mut self,page_id:u64)->Result<&mut DLLNode,RecordError>{
        match self.map.get(&page_id){
            Some(idx)=>{
                match self.dll.move_to_head(*idx){
                    Ok(_)=>{},
                    Err(_)=>return Err(RecordError::RecordMismatch),
                };
                let node : &mut DLLNode=&mut self.dll.nodes[*idx];
                return Ok(node)
            }
            None=>{
                return Err(RecordError::RecordAbsent)
            }
        };
    }
    pub fn delete(&mut self, page_id: u64) -> Result<(), CorruptedDataError> {
        let idx = match self.map.remove(&page_id) {
            Some(idx) => idx,
            None => return Ok(()),
        };
        self.dll.delete_node(idx)
    }
    pub fn set_new(&mut self,page : Page,db_file:&DatabaseFile)->Result<(),CorruptedDataError>{
        let id=page.header.page_id;
        match self.dll.add_node(page,db_file,&mut self.map){
            Ok(_)=>{}
            Err(e)=>return Err(e),
        };
        let idx=match self.dll.head{
            Some(t)=>t,
            None=>return Err(CorruptedDataError),
        };
        self.map.insert(id,idx);
        Ok(())
    }
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
    fn add_node(&mut self,page : Page,db_file:&DatabaseFile,map:&mut HashMap<u64,usize>)->Result<(),CorruptedDataError>{
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
        match self.shrink(db_file,map){
            Ok(())=>{},
            Err(e)=>return Err(e),
        };
        Ok(())
    }
    pub fn shrink(&mut self,db_file : &DatabaseFile,map:&mut HashMap<u64,usize>)->Result<(),CorruptedDataError>{
        while self.nodes.len()-self.trash.len()>self.capacity{
            let idx=match self.pop_tail(db_file){
                Ok(v)=>v,
                Err(_)=>return Err(CorruptedDataError),
            };
            let idx=self.nodes[idx].page.header.page_id;
            map.remove(&idx);
        }
        Ok(())
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
    pub fn pop_tail(&mut self,db_file:&DatabaseFile)->Result<usize,CorruptedDataError>{
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
            return Ok(tail);
        }
        else if (tail==sentinel && head!=sentinel)||(tail!=sentinel && head==sentinel){
            return Err(CorruptedDataError);
        }
        else if tail==head{
            self.tail=None;
            self.head=None;
        }
        else{
            let prev_idx=match self.nodes[tail].prev{
                Some(p)=>p,
                None=>return Err(CorruptedDataError),
            };
            self.nodes[prev_idx].next=None;
            self.tail=Some(prev_idx);
        }
        let x=&mut self.nodes[tail];
        match x.page.flush(db_file){
            Ok(())=>{},
            Err(_)=>return Err(CorruptedDataError),
        };
        self.trash.push(tail);
        Ok(tail)
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
