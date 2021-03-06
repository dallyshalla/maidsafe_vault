// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

#![allow(dead_code)]

use routing::NameType;
use routing::sendable::Sendable;
use routing::types::{GROUP_SIZE};
use routing::node_interface::MethodCall;
use std::collections::HashMap;
use cbor;
use rustc_serialize::{Encodable};

type Identity = NameType; // name of the chunk
type PmidNode = NameType;
pub type PmidNodes = Vec<PmidNode>;

#[derive(RustcEncodable, RustcDecodable, PartialEq, Eq, Debug)]
pub struct DataManagerSendable {
    name: NameType,
    tag: u64,
    data_holders: PmidNodes,
    preserialised_content: Vec<u8>,
    has_preserialised_content: bool,
}

impl DataManagerSendable {
    pub fn new(name: NameType, data_holders: PmidNodes) -> DataManagerSendable {
        DataManagerSendable {
            name: name,
            tag: 206, // FIXME : Change once the tag is freezed
            data_holders: data_holders,
            preserialised_content: Vec::new(),
            has_preserialised_content: false,
        }
    }

    pub fn with_content(name: NameType, preserialised_content: Vec<u8>) -> DataManagerSendable {
        DataManagerSendable {
            name: name,
            tag: 206, // FIXME : Change once the tag is freezed
            data_holders: PmidNodes::new(),
            preserialised_content: preserialised_content,
            has_preserialised_content: true,
        }
    }

    pub fn get_data_holders(&self) -> PmidNodes {
        self.data_holders.clone()
    }
}

impl Sendable for DataManagerSendable {
    fn name(&self) -> NameType {
        self.name.clone()
    }

    fn type_tag(&self) -> u64 {
        self.tag.clone()
    }

    fn serialised_contents(&self) -> Vec<u8> {
        if self.has_preserialised_content {
            self.preserialised_content.clone()
        } else {
            let mut e = cbor::Encoder::from_memory();
            e.encode(&[&self]).unwrap();
            e.into_bytes()
        }
    }

    fn refresh(&self)->bool {
        true
    }

    fn merge(&self, responses: Vec<Box<Sendable>>) -> Option<Box<Sendable>> {
        if responses.len() == GROUP_SIZE as usize - 1 {
            return None;
        }
        let mut tmp_wrapper: DataManagerSendable;
        let mut stats = Vec::<(PmidNodes, u64)>::new();
        for it in responses.iter() {
            let mut d = cbor::Decoder::from_bytes(it.serialised_contents());
            tmp_wrapper = d.decode().next().unwrap().unwrap();
            let mut push_in_vec = false;
            {
                let find_res = stats.iter_mut().find(|a| a.0 == tmp_wrapper.get_data_holders());
                if find_res.is_some() {
                    find_res.unwrap().1 += 1
                } else {
                    push_in_vec = true;
                }
            }

            if push_in_vec {
                stats.push((tmp_wrapper.get_data_holders(), 1));
            }
        }
        stats.sort_by(|a, b| b.1.cmp(&a.1));
        let (pmids, count) = stats[0].clone();
        if count < (GROUP_SIZE as u64 + 1) / 2 {
            return Some(Box::new(DataManagerSendable::new(NameType([0u8;64]), pmids)));
        }
        None
    }

}


pub struct DataManagerDatabase {
  storage : HashMap<Identity, PmidNodes>,
  pub close_grp_from_churn: Vec<NameType>,
  pub temp_storage_after_churn: HashMap<NameType, PmidNodes>,
}

impl DataManagerDatabase {
    pub fn new () -> DataManagerDatabase {
        DataManagerDatabase {
            storage: HashMap::with_capacity(10000),
            close_grp_from_churn: Vec::new(),
            temp_storage_after_churn: HashMap::new(),
        }
    }

    pub fn exist(&mut self, name : &Identity) -> bool {
        self.storage.contains_key(name)
    }

    pub fn put_pmid_nodes(&mut self, name : &Identity, pmid_nodes: PmidNodes) {
        self.storage.entry(name.clone()).or_insert(pmid_nodes.clone());
    }

    pub fn add_pmid_node(&mut self, name : &Identity, pmid_node: PmidNode) {
        let nodes = self.storage.entry(name.clone()).or_insert(vec![pmid_node.clone()]);
        if !nodes.contains(&pmid_node) {
            nodes.push(pmid_node);
        }
    }

    pub fn remove_pmid_node(&mut self, name : &Identity, pmid_node: PmidNode) {
        if !self.storage.contains_key(name) {
            return;
        }
        let nodes = self.storage.entry(name.clone()).or_insert(vec![]);
        for i in 0..nodes.len() {
            if nodes[i] == pmid_node {
              nodes.remove(i);
              break;
            }
        }
    }

    pub fn get_pmid_nodes(&mut self, name : &Identity) -> PmidNodes {
        let entry = self.storage.get(&name);
        if entry.is_some() {
            entry.unwrap().clone()
        } else {
            Vec::<PmidNode>::new()
        }
    }


    pub fn handle_account_transfer(&mut self, account_wrapper : &DataManagerSendable) {
        // TODO: Assuming the incoming merged account entry has the priority and shall also be trusted first
        let _ = self.storage.remove(&account_wrapper.name());
        self.storage.insert(account_wrapper.name(), account_wrapper.get_data_holders());
    }

    pub fn retrieve_all_and_reset(&mut self, close_group: &mut Vec<NameType>) -> Vec<MethodCall> {
        assert!(close_group.len() >= 3);
        self.temp_storage_after_churn = self.storage.clone();
        let mut close_grp_already_stored = false;

        for it in self.storage.iter_mut() {
            let mut new_pmid_nodes = Vec::<NameType>::with_capacity(it.1.len());
            for vec_it in it.1.iter() {
                if close_group.iter().find(|a| **a == *vec_it).is_some() {
                    new_pmid_nodes.push(vec_it.clone());
                }
            }

            if new_pmid_nodes.len() < 3 && !close_grp_already_stored {
                self.close_grp_from_churn = close_group.clone();
                close_grp_already_stored = true;
            }
            *it.1 = new_pmid_nodes;
        }

        let data: Vec<_> = self.storage.drain().collect();
        let mut actions = Vec::<MethodCall>::new();
        for element in data {
            if self.temp_storage_after_churn.get(&element.0).unwrap().len() < 3 {
                actions.push(MethodCall::Get {
                    type_id: 206, //TODO Get type_tag correct
                    name: element.0.clone(),
                });
            }
            actions.push(MethodCall::Refresh {
                content: Box::new(DataManagerSendable::new(element.0, element.1)),
            });
        }
        actions
    }
}

#[cfg(test)]
mod test {
  use super::*;
  use maidsafe_types::ImmutableData;
  use routing::NameType;
  use routing::types::{generate_random_vec_u8};
  use routing::test_utils::Random;
  use routing::sendable::Sendable;

  #[test]
  fn exist() {
    let mut db = DataManagerDatabase::new();
    let value = generate_random_vec_u8(1024);
    let data = ImmutableData::new(value);
    let mut pmid_nodes : Vec<NameType> = vec![];

    for _ in 0..4 {
      pmid_nodes.push(Random::generate_random());
    }

    let data_name = data.name();
    assert_eq!(db.exist(&data_name), false);
    db.put_pmid_nodes(&data_name, pmid_nodes);
    assert_eq!(db.exist(&data_name), true);
  }

  #[test]
  fn put() {
    let mut db = DataManagerDatabase::new();
    let value = generate_random_vec_u8(1024);
    let data = ImmutableData::new(value);
    let data_name = data.name();
    let mut pmid_nodes : Vec<NameType> = vec![];

    for _ in 0..4 {
      pmid_nodes.push(Random::generate_random());
    }

    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result.len(), 0);

    db.put_pmid_nodes(&data_name, pmid_nodes.clone());

    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result.len(), pmid_nodes.len());
  }

  #[test]
  fn remove_pmid() {
    let mut db = DataManagerDatabase::new();
    let value = generate_random_vec_u8(1024);
    let data = ImmutableData::new(value);
    let data_name = data.name();
    let mut pmid_nodes : Vec<NameType> = vec![];

    for _ in 0..4 {
      pmid_nodes.push(Random::generate_random());
    }

    db.put_pmid_nodes(&data_name, pmid_nodes.clone());
    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result, pmid_nodes);

    db.remove_pmid_node(&data_name, pmid_nodes[0].clone());

    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result.len(), 3);
    for index in 0..result.len() {
      assert!(result[index] != pmid_nodes[0]);
    }
  }

  #[test]
  fn replace_pmids() {
    let mut db = DataManagerDatabase::new();
    let value = generate_random_vec_u8(1024);
    let data = ImmutableData::new(value);
    let data_name = data.name();
    let mut pmid_nodes : Vec<NameType> = vec![];
    let mut new_pmid_nodes : Vec<NameType> = vec![];

    for _ in 0..4 {
      pmid_nodes.push(Random::generate_random());
      new_pmid_nodes.push(Random::generate_random());
    }

    db.put_pmid_nodes(&data_name, pmid_nodes.clone());
    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result, pmid_nodes);
    assert!(result != new_pmid_nodes);

    for index in 0..4 {
      db.remove_pmid_node(&data_name, pmid_nodes[index].clone());
      db.add_pmid_node(&data_name, new_pmid_nodes[index].clone());
    }

    let result = db.get_pmid_nodes(&data_name);
    assert_eq!(result, new_pmid_nodes);
    assert!(result != pmid_nodes);
  }

  #[test]
  fn handle_account_transfer() {
    let mut db = DataManagerDatabase::new();
    let value = generate_random_vec_u8(1024);
    let data = ImmutableData::new(value);
    let data_name = data.name();
    let mut pmid_nodes : Vec<NameType> = vec![];

    for _ in 0..4 {
      pmid_nodes.push(Random::generate_random());
    }
    db.put_pmid_nodes(&data_name, pmid_nodes.clone());
    assert_eq!(db.get_pmid_nodes(&data_name).len(), pmid_nodes.len());

    db.handle_account_transfer(&DataManagerSendable::new(data_name.clone(), vec![]));
    assert_eq!(db.get_pmid_nodes(&data_name).len(), 0);
  }
}
