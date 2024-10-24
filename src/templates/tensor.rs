use std::{
    marker::PhantomData,
    ops::{Add, AddAssign, Div, Mul, Sub},
    str::FromStr,
};

use dam::types::{DAMType, StaticallySized};
use itertools::Itertools;
use std::ops::Neg;

use ndarray::{
    Array, Array2, ArrayBase, Axis, CowArray, CowRepr, Dim, Dimension, IntoDimension, Ix1, Ix2,
    OwnedRepr, RawData, ShapeBuilder,
};

#[derive(Clone, PartialEq, Debug)]
pub struct Tensor<'a, ValType: DAMType, Dim: ndarray::Dimension, const N: usize> {
    pub data: ndarray::CowArray<'a, ValType, Dim>,
}

pub struct PrimitiveType<T: DAMType> {
    pub _marker: PhantomData<T>,
}

impl<T: DAMType> PrimitiveType<T> {
    pub fn new() -> PrimitiveType<T> {
        PrimitiveType::<T> {
            _marker: PhantomData,
        }
    }
}

pub trait Adapter<T> {
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<T>;
}

impl<T: std::str::FromStr> Adapter<T> for PrimitiveType<T>
where
    T: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<T> {
        iter.flat_map(|line| line.parse::<T>()) // ignores Err variant from Result of str.parse
            .collect()
    }
}

impl<'a, A, const N: usize> Adapter<Tensor<'a, A, Ix1, N>> for PrimitiveType<Tensor<'a, A, Ix1, N>>
where
    A: DAMType + FromStr,
    Tensor<'a, A, Dim<[usize; 1]>, N>: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<Tensor<'a, A, Ix1, N>> {
        let mut out_vec = vec![];
        let float_iter = iter.flat_map(|line| line.parse::<A>());
        for chunk in &float_iter.chunks(N) {
            out_vec.push(Tensor::<'a, A, Ix1, N> {
                data: CowArray::from(Array::from_vec(chunk.into_iter().collect::<Vec<_>>())),
            });
        }
        out_vec
    }
}

impl<'a, A, const N: usize> Adapter<Tensor<'a, A, Ix2, N>> for PrimitiveType<Tensor<'a, A, Ix2, N>>
where
    A: DAMType + FromStr,
    Tensor<'a, A, Dim<[usize; 2]>, N>: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<Tensor<'a, A, Ix2, N>> {
        let mut out_vec = vec![];
        let float_iter = iter.flat_map(|line| line.parse::<A>());
        for chunk in &float_iter.chunks(N * N) {
            out_vec.push(Tensor::<'a, A, Ix2, N> {
                data: CowArray::from(
                    Array2::from_shape_vec((N, N).f(), chunk.into_iter().collect::<Vec<_>>())
                        .unwrap(),
                ),
            });
        }
        out_vec
    }
}

impl<'a, A, D, const N: usize> Mul for Tensor<'a, A, D, N>
where
    A: DAMType,
    D: Dimension,
    ArrayBase<CowRepr<'a, A>, D>: From<<ArrayBase<OwnedRepr<A>, D> as Mul>::Output>,
    Array<A, D>: std::ops::Mul,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let data = self.data.to_owned().mul(rhs.data.to_owned());
        Self {
            data: CowArray::from(data),
        }
    }
}

impl<'a, A, D, const N: usize> Div for Tensor<'a, A, D, N>
where
    A: DAMType,
    D: Dimension,
    ArrayBase<CowRepr<'a, A>, D>: From<<ArrayBase<OwnedRepr<A>, D> as Div>::Output>,
    Array<A, D>: std::ops::Div,
{
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        let res = self.data.to_owned().div(rhs.data.to_owned());
        Self {
            data: CowArray::from(res),
        }
    }
}

impl<'a, A: DAMType + std::cmp::PartialEq + PartialOrd, D: ndarray::Dimension, const N: usize>
    PartialOrd for Tensor<'a, A, D, N>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.data
            .to_owned()
            .into_raw_vec()
            .partial_cmp(&other.data.to_owned().into_raw_vec())
    }
}

// impl<'a, A, D, const N: usize> Sub for Tensor<'a, A, D, N>
// where
//     // A: DAMType,
//     A: DAMType + StaticallySized + num::Zero + num::One,
//     D: Dimension + 'a,
//     CowArray<'a, A, D>: Sub<Output = CowArray<'a, A, D>>,
// {
//     type Output = Self;

//     fn sub(self, rhs: Self) -> Self::Output {
//         let data = self.data.to_owned() - rhs.data.to_owned();
//         Tensor::<'a, A, D, N> {
//             data: self.data.sub(rhs.data),
//         }
//     }
// }

impl<'a, A, D, const N: usize> Sub for Tensor<'a, A, D, N>
where
    A: DAMType + StaticallySized + num::Zero + num::One + std::ops::Neg<Output = A>,
    D: Dimension + 'a,
    &'a ArrayBase<OwnedRepr<A>, D>:
        Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>,
{
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        let scalar = -A::one();
        let data = rhs.data.map(|x| x.clone() * scalar.clone()).to_owned() + self.data.to_owned();
        // println!("SUB: {:?}", self.clone());
        // println!("SUB: {:?}", rhs.clone());
        // println!("SUB: {:?}", data.clone());
        Tensor::<'a, A, D, N> { data: data.into() }
    }
}

impl<'a, A, D, const N: usize> Neg for Tensor<'a, A, D, N>
where
    A: DAMType + std::ops::Mul + num::One + Neg<Output = A>,
    // A: DAMType + StaticallySized + num::Zero + num::One,
    D: Dimension,
    CowArray<'a, A, D>: Sub<Output = CowArray<'a, A, D>>,
    ArrayBase<CowRepr<'a, A>, D>: From<ArrayBase<OwnedRepr<f32>, D>>,
    ArrayBase<CowRepr<'a, A>, D>: From<ArrayBase<OwnedRepr<<A as Mul>::Output>, D>>,
{
    type Output = Tensor<'a, A, D, N>;

    fn neg(self) -> Self::Output {
        let scalar = -A::one();
        Tensor::<'a, A, D, N> {
            data: self.data.map(|x| x.clone() * scalar.clone()).into(),
        }
    }
}

impl<'a, A, D, const N: usize> Add for Tensor<'a, A, D, N>
where
    A: DAMType + StaticallySized + num::Zero + num::One,
    D: Dimension + 'a,
    &'a ArrayBase<OwnedRepr<A>, D>:
        Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>,
{
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        let data = self.data.to_owned() + rhs.data.to_owned();
        Tensor::<'a, A, D, N> { data: data.into() }
    }
}

impl<'a, A, D, const N: usize> AddAssign for Tensor<'a, A, D, N>
where
    A: DAMType + num::One + AddAssign,
    D: Dimension + 'a,
    &'a ArrayBase<OwnedRepr<A>, D>:
        Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>,
{
    fn add_assign(&mut self, rhs: Self) {
        self.data.to_owned().add_assign(&rhs.data.to_owned());
    }
}

impl<'a, A, D, const N: usize> DAMType for Tensor<'a, A, D, N>
where
    A: DAMType + StaticallySized,
    D: Dimension,
    Tensor<'a, A, D, N>: Default,
{
    fn dam_size(&self) -> usize {
        self.data.dim().into_dimension().size() * A::SIZE
    }
}

impl<'a, A, D, const N: usize> Tensor<'a, A, D, N>
where
    A: DAMType + StaticallySized,
    D: Dimension,
{
    fn size(&self) -> usize {
        self.data.dim().into_dimension().size()
    }
}

impl<'a, A, const N: usize> Default for Tensor<'a, A, Ix1, N>
where
    A: DAMType,
    Ix1: Dimension,
{
    fn default() -> Self {
        Tensor::<'a, A, Ix1, N> {
            data: CowArray::from(Array::from_vec(vec![A::default(); N])),
        }
    }
}

impl<'a, A, const N: usize> Default for Tensor<'a, A, Ix2, N>
where
    A: DAMType,
    Ix2: Dimension,
{
    fn default() -> Self {
        Tensor::<'a, A, Ix2, N> {
            data: CowArray::from(
                Array2::from_shape_vec((N, N).f(), vec![A::default(); N * N]).unwrap(),
            ),
        }
    }
}

impl<'a, A, const N: usize> num::Zero for Tensor<'a, A, Ix2, N>
where
    A: DAMType + num::Zero + dam::types::StaticallySized + num::One,
    Ix2: Dimension,
{
    fn set_zero(&mut self) {
        *self = num::Zero::zero();
    }

    fn zero() -> Self {
        Tensor::<'a, A, Ix2, N> {
            data: CowArray::from(
                Array2::from_shape_vec((N, N).f(), vec![A::default(); N * N]).unwrap(),
            ),
        }
    }

    fn is_zero(&self) -> bool {
        todo!()
    }
}

impl<'a, A: DAMType, const N: usize> std::fmt::Display for Tensor<'a, A, Ix2, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        //         for elem in self.data.iter() {
        //             writeln!(f, "{:?}", elem).unwrap();
        //         }
        for row in self.data.axis_iter(Axis(0)) {
            for (j, value) in row.iter().enumerate() {
                writeln!(f, "{:?}", value).unwrap();
            }
        }
        Ok(())
    }
}

impl<'a, A, const N: usize> num::One for Tensor<'a, A, Ix2, N>
where
    A: DAMType + num::Zero + dam::types::StaticallySized + num::One,
    Ix2: Dimension,
{
    fn one() -> Self {
        Tensor::<'a, A, Ix2, N> {
            data: CowArray::from(
                Array2::from_shape_vec((N, N).f(), vec![A::default(); N * N]).unwrap(),
            ),
        }
    }
}

impl<'a, A, const N: usize> Tensor<'a, A, Ix2, N>
where
    A: DAMType + dam::types::StaticallySized,
{
    pub fn new(raw_data: Array2<A>) -> Tensor<'a, A, Ix2, N> {
        Tensor::<'a, A, Ix2, N> {
            data: CowArray::from(raw_data),
        }
    }
}

// unsafe impl<'a, A, const N: usize> RawData for Tensor<'a, A, Ix2, N>
// where
//     A: DAMType + num::Zero + dam::types::StaticallySized + num::One,
//     Ix2: Dimension,
// {
//     type Elem = Self;

//     fn _data_slice(&self) -> Option<&[Self::Elem]> {
//         todo!()
//     }

//     fn _is_pointer_inbounds(&self, ptr: *const Self::Elem) -> bool {
//         todo!()
//     }

//     // #[doc = r" This trait is private to implement; this method exists to make it"]
//     // #[doc = r" impossible to implement outside the crate."]
//     // #[doc(hidden)]
//     // fn __private__(&self) {
//     //     todo!()
//     // }
// }
