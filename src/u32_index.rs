/*! 32 bit index type macro.

This module defines a macro [crate::define_u32_index].
This is useful for saving memory as we store index into vec in many
locations. Normal index usize takes 8 bytes, whereas u32 takes 4 bytes.

*/

/** Define a custom type for u32 that works as index without casting.

Example:

    use gleisbau::define_u32_index;

    define_u32_index!(
        /// My index
        pub struct SmallIndex;
    );
    # struct SomeStruct { data: usize, };
    # fn get_data() -> Vec<SomeStruct> { vec![ SomeStruct {data: 0} ] }
    let my_vec: Vec<SomeStruct> = get_data();
    let small_inx = SmallIndex::new(0);

    // Use helper function with Vec.get
    let result_opt = my_vec.get(small_inx.index());

    // Works directly with brackets
    let result = &my_vec[small_inx];

*/
#[macro_export]
macro_rules! define_u32_index {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        $vis struct $name(pub u32);

        impl $name {
            #[inline]
            pub const fn new(index: usize) -> Self {
                Self(index as u32)
            }

            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        // Implement for slices
        impl<T> ::std::ops::Index<$name> for [T] {
            type Output = T;

            #[inline]
            fn index(&self, index: $name) -> &Self::Output {
                &self[index.0 as usize]
            }
        }

        impl<T> ::std::ops::IndexMut<$name> for [T] {
            #[inline]
            fn index_mut(&mut self, index: $name) -> &mut Self::Output {
                &mut self[index.0 as usize]
            }
        }

        // Explicit Vec implementations
        impl<T> ::std::ops::Index<$name> for Vec<T> {
            type Output = T;

            #[inline]
            fn index(&self, index: $name) -> &Self::Output {
                &self[index.0 as usize]
            }
        }

        impl<T> ::std::ops::IndexMut<$name> for Vec<T> {
            #[inline]
            fn index_mut(&mut self, index: $name) -> &mut Self::Output {
                &mut self[index.0 as usize]
            }
        }

        // Convert from this type to usize
        impl ::core::convert::From<$name> for ::core::primitive::usize {
            #[inline]
            fn from(index: $name) -> Self {
                index.0 as ::core::primitive::usize
            }
        }

        // Convert from usize to this type
        impl ::core::convert::From<::core::primitive::usize> for $name {
            #[inline]
            fn from(index: ::core::primitive::usize) -> Self {
                Self(index as ::core::primitive::u32)
            }
        }
    };
}
