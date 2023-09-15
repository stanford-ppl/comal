use std::{
    marker::PhantomData,
    ops::{Add, Mul, Sub},
    str::FromStr,
};

use dam_rs::types::{DAMType, StaticallySized};
use itertools::Itertools;
use ndarray::{
    Array, Array1, ArrayBase, CowArray, CowRepr, Dim, Dimension, IntoDimension, Ix1, LinalgScalar,
    Shape,
};
use num::{One, Zero};

#[derive(Clone, PartialEq, Debug)]
pub struct Tensor<'a, ValType: DAMType, Dim: ndarray::Dimension> {
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
    // fn parse(&self, iter: impl Iterator<Item = String>) -> Vec<T> {
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<T> {
        iter.flat_map(|line| line.parse::<T>()) // ignores Err variant from Result of str.parse
            .collect()
    }
}

impl<'a, A> Adapter<Tensor<'a, A, Ix1>> for PrimitiveType<Tensor<'a, A, Ix1>>
where
    A: DAMType + FromStr,
    Tensor<'a, A, Dim<[usize; 1]>>: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
    ) -> Vec<Tensor<'a, A, Ix1>> {
        let mut out_vec = vec![];
        let float_iter = iter.flat_map(|line| line.parse::<A>());
        for chunk in &float_iter.chunks(4) {
            out_vec.push(Tensor::<'a, A, Ix1> {
                data: CowArray::from(Array::from_vec(chunk.into_iter().collect::<Vec<_>>())),
            });
        }
        out_vec
    }
}

// impl<'a, A, D> Copy for Tensor<'a, A, D>
// where
//     A: PartialEq
//         + std::fmt::Debug
//         + Clone
//         + Default
//         + Sync
//         + Send
//         + StaticallySized
//         + num::Zero
//         + std::marker::Copy,
//     D: Dimension + std::marker::Copy,
// {
// }

// impl<'a, A, D> LinalgScalar for Tensor<'a, A, D>
// where
//     A: DAMType + dam_rs::types::StaticallySized,
//     D: Dimension,
// {
// }

// impl<'a, A, D> num_traits::identities::Zero for ArrayBase<CowRepr<'a, A>, D>
// where
//     A: DAMType,
//     D: Dimension,
// {
// }

// impl<'a, A, D> One for Tensor<'a, A, D>
// where
//     A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
//     D: Dimension,
//     // ArrayBase<CowRepr<'a, A>, D>: num_traits::identities::One, // Tensor<'a, A, D>: LinalgScalar,
// {
//     fn one() -> Self {
//         Tensor {
//             data: ArrayBase::ones(),
//         }
//     }

//     fn set_one(&mut self) {
//         *self = One::one();
//     }

//     fn is_one(&self) -> bool
//     where
//         Self: PartialEq,
//     {
//         *self == Self::one()
//     }
// }

// impl<'a, A, D> Zero for Tensor<'a, A, D>
// where
//     A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
//     D: Dimension,
//     ArrayBase<CowRepr<'a, A>, D>: Add<Output = ArrayBase<CowRepr<'a, A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
//     ArrayBase<CowRepr<'a, A>, D>: num_traits::identities::Zero, // Tensor<'a, A, D>: LinalgScalar,
// {
//     fn zero() -> Self {
//         Tensor {
//             data: ArrayBase::<CowRepr<'a, A>, D>::zero(),
//         }
//     }

//     fn is_zero(&self) -> bool
//     where
//         Self: PartialEq,
//     {
//         *self == Self::zero()
//     }
// }

impl<'a, A, D> Mul for Tensor<'a, A, D>
where
    A: DAMType,
    D: Dimension,
    // ArrayBase<CowRepr<'a, A>, D>: Mul<Output = ArrayBase<CowRepr<'a, A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
    // CowArray<'a, A, D>: Mul<Output = CowArray<'a, A, D>>, // Tensor<'a, A, D>: LinalgScalar,
    CowArray<'a, A, D>: LinalgScalar,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            data: self.data.mul(rhs.data),
            // data: self.data * rhs.data,
        }
    }
}

impl<'a, A, D> Sub for Tensor<'a, A, D>
where
    A: PartialEq
        + std::fmt::Debug
        + Clone
        + Default
        + Sync
        + Send
        + StaticallySized
        + num::Zero
        + ndarray::RawData,
    D: Dimension,
    // ArrayBase<CowRepr<'a, A>, D>: Sub<Output = ArrayBase<CowRepr<'a, A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
    CowArray<'a, A, D>: Sub<Output = CowArray<'a, A, D>>, // Tensor<'a, A, D>: LinalgScalar,
                                                          // CowArray<'a, A, D>: LinalgScalar,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Tensor::<'a, A, D> {
            // data: self.data.sub(rhs.data),
            data: self.data.sub(rhs.data),
            // data: CowArray::from(Array::from_vec(
            //     self.data
            //         .iter()
            //         .zip(rhs.data.iter())
            //         .map(|a| a.0 + b.1)
            //         .collect::<Vec<_>>(),
            // )),
        }
    }
}

impl<'a, A, D> Add for Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
    // ArrayBase<CowRepr<'a, A>, D>: Add<Output = ArrayBase<CowRepr<'a, A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
    CowArray<'a, A, D>: Add<Output = CowArray<'a, A, D>>, // Tensor<'a, A, D>: LinalgScalar,
                                                          // CowArray<'a, A, D>: ,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Tensor::<'a, A, D> {
            data: self.data.add(rhs.data),
            // data: self.data + rhs.data,
        }
    }
}

impl<'a, A, D> DAMType for Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
{
    fn dam_size(&self) -> usize {
        self.data.dim().into_dimension().size() * A::SIZE
    }
}

impl<'a, A, D> Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized,
    D: Dimension,
    // Tensor<'a, A, D>: LinalgScalar,
{
    fn size(&self) -> usize {
        self.data.dim().into_dimension().size()
    }
}

impl<'a, A, D> Default for Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
{
    fn default() -> Self {
        Tensor {
            data: CowArray::from(Array::zeros(Shape::from(D::default()))),
        }
    }
}
