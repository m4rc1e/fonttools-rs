use crate::error::{Error, Result};
use serde::de::{self, Deserialize, DeserializeSeed, SeqAccess, Visitor};
use std::convert::TryInto;
use std::mem;

pub struct Deserializer<'de> {
    // This string starts with the input data and characters are truncated off
    // the beginning as data is parsed.
    input: &'de [u8],
    ptr: usize,
}

impl<'de> Deserializer<'de> {
    pub fn from_bytes(input: &'de [u8]) -> Self {
        Deserializer { input, ptr: 0 }
    }
}

pub fn from_bytes<'a, T>(s: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_bytes(s);
    let t = T::deserialize(&mut deserializer)?;
    // if deserializer.input.is_empty() {
    Ok(t)
    // } else {
    // Err(Error::TrailingCharacters)
    // }
}

impl<'de> Deserializer<'de> {
    fn consume(&mut self, bytes: usize) -> Result<&'de [u8]> {
        if self.ptr + bytes > self.input.len() {
            Err(Error::Eof)
        } else {
            let subslice = &self.input[self.ptr..self.ptr + bytes];
            self.ptr += bytes;
            Ok(subslice)
        }
    }

    fn parse_bool(&mut self) -> Result<bool> {
        let b = self.consume(1)?;
        if b[0] > 0 {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

macro_rules! deserialize_number_type {
    ($func:ident, $type:ty, $visitor: ident) => {
        fn $func<V>(self, visitor: V) -> Result<V::Value>
        where
            V: Visitor<'de>,
        {
            let bytes: &[u8] = self.consume(mem::size_of::<$type>())?;
            let bytes_array: [u8; mem::size_of::<$type>()] =
                bytes.try_into().expect("Slice with incorrect length");
            let i = <$type>::from_be_bytes(bytes_array);
            visitor.$visitor(i)
        }
    };
}

impl<'de, 'a> SeqAccess<'de> for Deserializer<'de> {
    type Error = Error;
    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        let thing = seed.deserialize(self);
        thing.map(Some)
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeAnyNotSupported)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.parse_bool()?)
    }

    deserialize_number_type!(deserialize_i8, i8, visit_i8);
    deserialize_number_type!(deserialize_i16, i16, visit_i16);
    deserialize_number_type!(deserialize_i32, i32, visit_i32);
    deserialize_number_type!(deserialize_i64, i64, visit_i64);
    deserialize_number_type!(deserialize_u8, u8, visit_u8);
    deserialize_number_type!(deserialize_u16, u16, visit_u16);
    deserialize_number_type!(deserialize_u32, u32, visit_u32);
    deserialize_number_type!(deserialize_u64, u64, visit_u64);

    // Float parsing is stupidly hard.
    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // Float parsing is stupidly hard.
    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_string<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // The `Serializer` implementation on the previous page serialized byte
    // arrays as JSON arrays of bytes. Handle that representation here.
    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    // In Serde, unit means an anonymous value containing no data.
    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::ExpectedNull)
    }

    // Unit struct means a named value containing no data.
    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(EofChecking::new(&mut self))
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::ExpectedMap)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // println!(
        //     "Tying to deserialize an enum {:?}, variants {:?}",
        //     name, variants
        // );
        Err(Error::ExpectedEnum)
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }
}

struct EofChecking<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> EofChecking<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        EofChecking { de }
    }
}

impl<'de, 'a> SeqAccess<'de> for EofChecking<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        // println!(
        //     "Is there more? PTR = {:?} len = {:?}",
        //     self.de.ptr,
        //     self.de.input.len()
        // );
        if self.de.ptr >= self.de.input.len() {
            // println!("No, we hit the end");
            return Ok(None);
        }
        // println!("Yes, let's go...");
        seed.deserialize(&mut *self.de).map(Some)
    }
}

pub struct CountedDeserializer<'a, T: 'a> {
    len: usize,
    _phantom: &'a std::marker::PhantomData<T>,
}

impl<T> CountedDeserializer<'_, T> {
    pub fn with_len(len: usize) -> Self {
        CountedDeserializer {
            len,
            _phantom: &std::marker::PhantomData,
        }
    }
}

impl<'de, 'a, T> DeserializeSeed<'de> for CountedDeserializer<'a, T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct CountedDeserializerVisitor<'a, T: 'a> {
            len: usize,
            _phantom: &'a std::marker::PhantomData<T>,
        }

        impl<'de, 'a, T> Visitor<'de> for CountedDeserializerVisitor<'a, T>
        where
            T: Deserialize<'de>,
        {
            type Value = Vec<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "an array of data")
            }

            fn visit_seq<A>(mut self, mut seq: A) -> std::result::Result<Vec<T>, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut res = Vec::new();
                if self.len > 0 {
                    while let Some(elem) = seq.next_element()? {
                        res.push(elem);
                        self.len -= 1;
                        if self.len == 0 {
                            break;
                        }
                    }
                }
                Ok(res)
            }
        }

        deserializer.deserialize_seq(CountedDeserializerVisitor {
            len: self.len,
            _phantom: &std::marker::PhantomData,
        })
    }
}

#[macro_export]
macro_rules! deserialize_visitor {
    ($struct_name:ident, $visitor_name:ident, $item:item) => {
        struct $visitor_name {
            _phantom: std::marker::PhantomData<$struct_name>,
        }

        impl $visitor_name {
            fn new() -> Self {
                $visitor_name {
                    _phantom: std::marker::PhantomData,
                }
            }
        }

        impl<'de> Deserialize<'de> for $struct_name {
            fn deserialize<D>(d: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                d.deserialize_seq($visitor_name::new())
            }
        }

        impl<'de> Visitor<'de> for $visitor_name {
            type Value = $struct_name;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "A {:} structure", stringify!($struct_name))
            }
            $item
        }

    };
}

#[macro_export]
macro_rules! read_field {
    ($seq:ident, $type:ty, $name:expr) => {
        $seq.next_element::<$type>()?
            .ok_or_else(|| serde::de::Error::custom(format!("Expecting {:}", $name)))?;
    };
}

#[macro_export]
macro_rules! read_remainder {
    ($seq:ident, $name:expr) => {
        $seq.next_element::<Vec<u8>>()?
            .ok_or_else(|| serde::de::Error::custom(format!("Expecting {:}", $name)))?;
    };
}

#[macro_export]
macro_rules! read_field_counted {
    ($seq:ident, $count:expr, $name:expr) => {
        $seq.next_element_seed(otspec::de::CountedDeserializer::with_len($count as usize))?
            .ok_or_else(|| serde::de::Error::custom(format!("Expecting {:}", $name)))?;
    };
}

#[macro_export]
macro_rules! stateful_deserializer {
    ($struct_name:ty,
     $deserializer_name:ident,
     { $($element: ident: $ty:ty),+ },
     $visit_seq_implementation:item
     ) => {

#[allow(missing_docs)]
pub struct $deserializer_name {
    $( pub $element: $ty, )+
}

#[allow(missing_docs)]
impl<'de> DeserializeSeed<'de> for $deserializer_name {
    type Value = $struct_name;

    fn deserialize<D>(self, deserializer: D) ->
        std::result::Result<Self::Value, D::Error>
        where D: serde::de::Deserializer<'de>,
    {

        struct MyVisitor {
            $( pub $element: $ty, )+
        }

        impl<'de> Visitor<'de> for MyVisitor {
            type Value = $struct_name;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a {:?}", stringify!($struct_name))
            }

            $visit_seq_implementation

        }

        deserializer.deserialize_seq(MyVisitor {
            $( $element: self.$element, )+
        })

    }

}


    };
}
