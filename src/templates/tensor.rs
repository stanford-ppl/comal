use std::{
    marker::PhantomData,
    ops::{Add, Mul, Sub},
    str::FromStr,
};

use dam_rs::types::{DAMType, StaticallySized};
use itertools::Itertools;
use ndarray::ShapeBuilder;
use ndarray::{
    Array, Array1, ArrayBase, CowArray, CowRepr, Dim, Dimension, IntoDimension, Ix1, LinalgScalar,
    OwnedRepr, Shape,
};
use num::{One, Zero};

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
    // fn parse(&self, iter: impl Iterator<Item = String>) -> Vec<T> {
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

impl<'a, A, D, const N: usize> Mul for Tensor<'a, A, D, N>
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

impl<'a, A, D, const N: usize> Sub for Tensor<'a, A, D, N>
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
                                                          // &'a ArrayBase<OwnedRepr<A>, D>:
                                                          //     Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Tensor::<'a, A, D, N> {
            // data: self.data.sub(rhs.data),
            data: self.data.sub(rhs.data),
        } // data: CowArray::from(Array::from_vec(
          //     self.data
          //         .iter()
          //         .zip(rhs.data.iter())
          //         .map(|a| a.0 + b.1)
          //         .collect::<Vec<_>>(),
          // )),
    }
}

impl<'a, A, D, const N: usize> Add for Tensor<'a, A, D, N>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension + 'a,
    // ArrayBase<CowRepr<'a, A>, D>: Add<Output = ArrayBase<CowRepr<'a, A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
    // &'a CowArray<'a, A, D>: Add<&'a CowArray<'a, A, D>, Output = CowArray<'a, A, D>>, // Tensor<'a, A, D>: LinalgScalar,
    &'a ArrayBase<OwnedRepr<A>, D>:
        Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
                                                                                  // CowArray<'a, A, D>: ,
{
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        dbg!(self.data.clone());
        dbg!(rhs.data.clone());
        let data = self.data.to_owned() + rhs.data.to_owned();
        // dbg!(data.clone());
        Tensor::<'a, A, D, N> {
            // data: self.data.add(rhs.data),
            data: data.into(),
        }
    }
}

impl<'a, A, D, const N: usize> DAMType for Tensor<'a, A, D, N>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
    Tensor<'a, A, D, N>: Default,
{
    fn dam_size(&self) -> usize {
        self.data.dim().into_dimension().size() * A::SIZE
    }
}

impl<'a, A, D, const N: usize> Tensor<'a, A, D, N>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized,
    D: Dimension,
    // Tensor<'a, A, D>: LinalgScalar,
{
    fn size(&self) -> usize {
        self.data.dim().into_dimension().size()
    }
}

impl<'a, A, const N: usize> Default for Tensor<'a, A, Ix1, N>
where
    A: DAMType,
    // D: Dimension, // ArrayBase<OwnedRepr<A>, Ix1>: Zero, // Add<&'a ArrayBase<OwnedRepr<A>, D>, Output = ArrayBase<OwnedRepr<A>, D>>, // Tensor<'a, A, D>: LinalgScalar,
    Ix1: Dimension,
{
    fn default() -> Self {
        // let data = Array::zeros(Dim(N).into_dimension());

        Tensor::<'a, A, Ix1, N> {
            // data: CowArray::from(Array::zeros(Shape::from(Ix1(N)).into_shape())),
            // data: data.into(),
            // data: CowArray::from(Array::from_vec(chunk.into_iter().collect::<Vec<_>>())),
            data: CowArray::from(Array::from_vec(vec![A::default(); N])),
        }
    }
}
