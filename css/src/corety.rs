use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::props::basic::ColorU;

// Define a struct for debug messages
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutDebugMessage {
    pub message: AzString,
    pub location: AzString,
}

#[repr(C)]
pub struct AzString {
    pub vec: U8Vec,
}

impl_option!(
    AzString,
    OptionAzString,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

static DEFAULT_STR: &str = "";

impl Default for AzString {
    fn default() -> Self {
        DEFAULT_STR.into()
    }
}

impl<'a> From<&'a str> for AzString {
    fn from(s: &'a str) -> Self {
        s.to_string().into()
    }
}

impl AsRef<str> for AzString {
    fn as_ref<'a>(&'a self) -> &'a str {
        self.as_str()
    }
}

impl core::fmt::Debug for AzString {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl core::fmt::Display for AzString {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl AzString {
    #[inline]
    pub const fn from_const_str(s: &'static str) -> Self {
        Self {
            vec: U8Vec::from_const_slice(s.as_bytes()),
        }
    }

    #[inline]
    pub fn from_string(s: String) -> Self {
        Self {
            vec: U8Vec::from_vec(s.into_bytes()),
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.vec.as_ref()) }
    }

    /// NOTE: CLONES the memory if the memory is external or &'static
    /// Moves the memory out if the memory is library-allocated
    #[inline]
    pub fn clone_self(&self) -> Self {
        Self {
            vec: self.vec.clone_self(),
        }
    }

    #[inline]
    pub fn into_library_owned_string(self) -> String {
        match self.vec.destructor {
            U8VecDestructor::NoDestructor | U8VecDestructor::External(_) => {
                self.as_str().to_string()
            }
            U8VecDestructor::DefaultRust => {
                let m = core::mem::ManuallyDrop::new(self);
                unsafe { String::from_raw_parts(m.vec.ptr as *mut u8, m.vec.len, m.vec.cap) }
            }
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.vec.as_ref()
    }

    #[inline]
    pub fn into_bytes(self) -> U8Vec {
        let m = core::mem::ManuallyDrop::new(self);
        U8Vec {
            ptr: m.vec.ptr,
            len: m.vec.len,
            cap: m.vec.cap,
            destructor: m.vec.destructor,
        }
    }
}

impl From<String> for AzString {
    fn from(input: String) -> AzString {
        AzString::from_string(input)
    }
}

impl PartialOrd for AzString {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        self.as_str().partial_cmp(rhs.as_str())
    }
}

impl Ord for AzString {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        self.as_str().cmp(rhs.as_str())
    }
}

impl Clone for AzString {
    fn clone(&self) -> Self {
        self.clone_self()
    }
}

impl PartialEq for AzString {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_str().eq(rhs.as_str())
    }
}

impl Eq for AzString {}

impl core::hash::Hash for AzString {
    fn hash<H>(&self, state: &mut H)
    where
        H: core::hash::Hasher,
    {
        self.as_str().hash(state)
    }
}

impl core::ops::Deref for AzString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl_vec!(u8, U8Vec, U8VecDestructor);
impl_vec_debug!(u8, U8Vec);
impl_vec_partialord!(u8, U8Vec);
impl_vec_ord!(u8, U8Vec);
impl_vec_clone!(u8, U8Vec, U8VecDestructor);
impl_vec_partialeq!(u8, U8Vec);
impl_vec_eq!(u8, U8Vec);
impl_vec_hash!(u8, U8Vec);

impl_option!(
    U8Vec,
    OptionU8Vec,
    copy = false,
    [Debug, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]
);

impl_vec!(u16, U16Vec, U16VecDestructor);
impl_vec_debug!(u16, U16Vec);
impl_vec_partialord!(u16, U16Vec);
impl_vec_ord!(u16, U16Vec);
impl_vec_clone!(u16, U16Vec, U16VecDestructor);
impl_vec_partialeq!(u16, U16Vec);
impl_vec_eq!(u16, U16Vec);
impl_vec_hash!(u16, U16Vec);

impl_vec!(f32, F32Vec, F32VecDestructor);
impl_vec_debug!(f32, F32Vec);
impl_vec_partialord!(f32, F32Vec);
impl_vec_clone!(f32, F32Vec, F32VecDestructor);
impl_vec_partialeq!(f32, F32Vec);

// Vec<char>
impl_vec!(u32, U32Vec, U32VecDestructor);
impl_vec_mut!(u32, U32Vec);
impl_vec_debug!(u32, U32Vec);
impl_vec_partialord!(u32, U32Vec);
impl_vec_ord!(u32, U32Vec);
impl_vec_clone!(u32, U32Vec, U32VecDestructor);
impl_vec_partialeq!(u32, U32Vec);
impl_vec_eq!(u32, U32Vec);
impl_vec_hash!(u32, U32Vec);

impl_vec!(AzString, StringVec, StringVecDestructor);
impl_vec_debug!(AzString, StringVec);
impl_vec_partialord!(AzString, StringVec);
impl_vec_ord!(AzString, StringVec);
impl_vec_clone!(AzString, StringVec, StringVecDestructor);
impl_vec_partialeq!(AzString, StringVec);
impl_vec_eq!(AzString, StringVec);
impl_vec_hash!(AzString, StringVec);

impl From<Vec<String>> for StringVec {
    fn from(v: Vec<String>) -> StringVec {
        let new_v: Vec<AzString> = v.into_iter().map(|s| s.into()).collect();
        new_v.into()
    }
}

impl_option!(
    StringVec,
    OptionStringVec,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Hash]
);

impl_option!(
    u16,
    OptionU16,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    u32,
    OptionU32,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    i16,
    OptionI16,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    i32,
    OptionI32,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(f32, OptionF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
impl_option!(f64, OptionF64, [Debug, Copy, Clone, PartialEq, PartialOrd]);
