use crate::otvar::{
    Delta, PackedDeltasDeserializer, PackedPoints, TupleIndexFlags, TupleVariationHeader,
    TupleVariationHeaderDeserializer,
};
use otspec::types::*;
use otspec::{read_field, stateful_deserializer};
use serde::de::DeserializeSeed;
use serde::de::SeqAccess;
use serde::de::Visitor;
use std::collections::VecDeque;

#[derive(Debug, PartialEq)]
pub struct TupleVariationStore(pub Vec<(TupleVariationHeader, Vec<Delta>)>);

stateful_deserializer!(
    TupleVariationStore,
    TupleVariationStoreDeserializer,
    {
        axis_count: uint16,
        is_gvar: bool,
        point_count: uint16
    },
    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<TupleVariationStore, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let packed_count = read_field!(seq, uint16, "a packed count");
        let count = packed_count & 0x0FFF;
        let points_are_shared = (packed_count & 0x8000) != 0;
        let mut shared_points = vec![];
        let _data_offset = read_field!(seq, uint16, "a data offset");
        let mut headers: Vec<TupleVariationHeader> = vec![];
        let mut variations: Vec<(TupleVariationHeader, Vec<Delta>)> = vec![];
        for _ in 0..count {
            headers.push(
                seq.next_element_seed(TupleVariationHeaderDeserializer {
                    axis_count: self.axis_count,
                })?
                .unwrap(),
            );
        }
        if points_are_shared {
            shared_points = match read_field!(seq, PackedPoints, "packed points").points {
                Some(pts) => pts,
                None =>  (0..self.point_count).collect()
            };
        }

        for header in headers {
            let mut points_for_this_header: VecDeque<u16>;
            /* Private points? */
            if header
                .flags
                .contains(TupleIndexFlags::PRIVATE_POINT_NUMBERS)
            {
                let private_points = read_field!(seq, PackedPoints, "packed points");
                if private_points.points.is_some() {
                    points_for_this_header = private_points.points.unwrap().clone().into();
                } else {
                    points_for_this_header =  (0..self.point_count).collect();
                }
            } else {
                points_for_this_header = shared_points.clone().into();
            }
            let mut deltas:VecDeque<Delta> = if self.is_gvar {
                let packed_x = seq.next_element_seed(PackedDeltasDeserializer { num_points: points_for_this_header.len() })?.unwrap().0;
                let packed_y = seq.next_element_seed(PackedDeltasDeserializer { num_points: points_for_this_header.len() })?.unwrap().0;
                packed_x.iter().zip(packed_y.iter()).map(|(x,y)| Delta::Delta2D((*x,*y)) ).collect()
            } else {
                let packed = seq.next_element_seed(PackedDeltasDeserializer { num_points: points_for_this_header.len() })?.unwrap().0;
                packed.iter().map(|x| Delta::Delta1D(*x) ).collect()
            };
            let mut all_deltas:Vec<Delta> = vec![];
            for i in 0..self.point_count {
                if !points_for_this_header.is_empty() && i == points_for_this_header[0] {
                    all_deltas.push(deltas.pop_front().unwrap());
                    points_for_this_header.pop_front();
                }
            }
            variations.push( (header, all_deltas))
        }

        Ok(TupleVariationStore(variations))
    }
);