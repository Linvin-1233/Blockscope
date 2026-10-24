use std::collections::BTreeMap;

/// NBT tag identifiers as stored in the binary format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NbtTagType {
    End = 0,
    Byte = 1,
    Short = 2,
    Int = 3,
    Long = 4,
    Float = 5,
    Double = 6,
    ByteArray = 7,
    String = 8,
    List = 9,
    Compound = 10,
    IntArray = 11,
    LongArray = 12,
}

impl NbtTagType {
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::End),
            1 => Some(Self::Byte),
            2 => Some(Self::Short),
            3 => Some(Self::Int),
            4 => Some(Self::Long),
            5 => Some(Self::Float),
            6 => Some(Self::Double),
            7 => Some(Self::ByteArray),
            8 => Some(Self::String),
            9 => Some(Self::List),
            10 => Some(Self::Compound),
            11 => Some(Self::IntArray),
            12 => Some(Self::LongArray),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::End => "end",
            Self::Byte => "byte",
            Self::Short => "short",
            Self::Int => "int",
            Self::Long => "long",
            Self::Float => "float",
            Self::Double => "double",
            Self::ByteArray => "byte_array",
            Self::String => "string",
            Self::List => "list",
            Self::Compound => "compound",
            Self::IntArray => "int_array",
            Self::LongArray => "long_array",
        }
    }
}

/// A decoded NBT value.
#[derive(Debug, Clone, PartialEq)]
pub enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(NbtList),
    Compound(NbtCompound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NbtValue {
    pub const fn tag_type(&self) -> NbtTagType {
        match self {
            Self::Byte(_) => NbtTagType::Byte,
            Self::Short(_) => NbtTagType::Short,
            Self::Int(_) => NbtTagType::Int,
            Self::Long(_) => NbtTagType::Long,
            Self::Float(_) => NbtTagType::Float,
            Self::Double(_) => NbtTagType::Double,
            Self::ByteArray(_) => NbtTagType::ByteArray,
            Self::String(_) => NbtTagType::String,
            Self::List(_) => NbtTagType::List,
            Self::Compound(_) => NbtTagType::Compound,
            Self::IntArray(_) => NbtTagType::IntArray,
            Self::LongArray(_) => NbtTagType::LongArray,
        }
    }
}

/// A homogeneous NBT list. An empty list may carry `End` as its element type.
#[derive(Debug, Clone, PartialEq)]
pub struct NbtList {
    pub element_type: NbtTagType,
    pub values: Vec<NbtValue>,
}

impl NbtList {
    pub fn new(element_type: NbtTagType, values: Vec<NbtValue>) -> Self {
        Self {
            element_type,
            values,
        }
    }
}

/// A compound tag. Keys are sorted so decoded output is deterministic.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NbtCompound {
    pub entries: BTreeMap<String, NbtValue>,
}

impl NbtCompound {
    pub fn get(&self, key: &str) -> Option<&NbtValue> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: String, value: NbtValue) -> Option<NbtValue> {
        self.entries.insert(key, value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A decoded NBT root tag.
#[derive(Debug, Clone, PartialEq)]
pub struct NbtRoot {
    pub name: String,
    pub value: NbtValue,
}
