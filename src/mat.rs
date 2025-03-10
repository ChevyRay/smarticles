use std::{
    fmt::Display,
    ops::{Add, Index, IndexMut, Mul, Sub},
};

use rand::{prelude::Distribution, Rng};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mat2D<T> {
    num_rows: usize,
    num_columns: usize,
    vec: Vec<T>,
}

impl<T> Mat2D<T>
where
    T: Clone,
{
    pub fn from_rows(vec: Vec<T>, num_rows: usize, num_columns: usize) -> Self {
        assert_eq!(
            vec.len(),
            num_rows * num_columns,
            "vec does not match the number of rows and columns"
        );
        Self {
            num_rows,
            num_columns,
            vec,
        }
    }

    pub fn filled_with(value: T, num_rows: usize, num_columns: usize) -> Self {
        Self {
            num_rows,
            num_columns,
            vec: vec![value; num_rows * num_columns],
        }
    }

    // pub fn from_iter_rows<I>(iter: I, num_rows: usize, num_columns: usize) -> Self
    // where
    //     I: IntoIterator<Item = T>,
    // {
    //     Self {
    //         num_rows,
    //         num_columns,
    //         vec: Vec::from_iter(iter),
    //     }
    // }

    pub fn vec(&self) -> Vec<T> {
        self.vec.to_owned()
    }
    pub fn vec_mut(&mut self) -> &mut Vec<T> {
        &mut self.vec
    }

    pub fn num_rows(&self) -> usize {
        self.num_rows
    }
    pub fn num_columns(&self) -> usize {
        self.num_columns
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.num_columns, self.num_rows)
    }
}

impl<T> Mat2D<T>
where
    T: Clone + Send + Sync,
{
    pub fn transpose(&self) -> Self {
        Mat2D::from_rows(
            (0..self.num_columns)
                .into_par_iter()
                .flat_map(move |i| {
                    (0..self.num_rows)
                        .into_par_iter()
                        .map(move |j| self[(j, i)].to_owned())
                })
                .collect(),
            self.num_columns,
            self.num_rows,
        )
    }
}

impl Mat2D<f32> {
    pub fn random<D>(distribution: D, num_rows: usize, num_columns: usize) -> Self
    where
        D: Distribution<f32> + Clone,
    {
        let mut rng = rand::thread_rng();

        let mut vec = vec![0.; num_rows * num_columns];
        vec.fill_with(|| rng.sample::<f32, _>(distribution.clone()));

        Self {
            num_rows,
            num_columns,
            vec,
        }
    }

    pub fn map<T>(&self, function: T) -> Self
    where
        T: (Fn(f32) -> f32) + Sync + Send,
    {
        Mat2D::from_rows(
            self.vec().par_iter().map(move |x| function(*x)).collect(),
            self.num_rows,
            self.num_columns,
        )
    }

    pub fn elementwise_product(&self, rhs: &Self) -> Self {
        assert_eq!(
            self.num_rows, rhs.num_rows,
            "num rows must match when performing element-wise product"
        );
        assert_eq!(
            self.num_columns, rhs.num_columns,
            "num columns must match when performing element-wise product"
        );

        Mat2D::from_rows(
            (0..self.num_rows)
                .into_par_iter()
                .flat_map(move |i| {
                    (0..self.num_columns)
                        .into_par_iter()
                        .map(move |j| self[(i, j)] * rhs[(i, j)])
                })
                .collect(),
            self.num_rows,
            self.num_columns,
        )
    }
}

impl<T> Index<(usize, usize)> for Mat2D<T>
where
    T: Clone,
{
    type Output = T;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        assert!(index.0 < self.num_rows, "row index out of bounds");
        assert!(index.1 < self.num_columns, "column index out of bounds");
        &self.vec[index.1 + index.0 * self.num_columns]
    }
}

impl<T> IndexMut<(usize, usize)> for Mat2D<T>
where
    T: Clone,
{
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        assert!(index.0 < self.num_rows, "row index out of bounds");
        assert!(index.1 < self.num_columns, "column index out of bounds");
        &mut self.vec[index.1 + index.0 * self.num_columns]
    }
}

impl Display for Mat2D<f32> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.num_columns < 12 {
            for i in 0..self.num_rows {
                for j in 0..self.num_columns {
                    let s = if self[(i, j)].is_sign_negative() {
                        format!("{:6.3}", self[(i, j)])
                    } else {
                        format!("{:6.4}", self[(i, j)])
                    };

                    write!(
                        f,
                        "{}{}{}",
                        if j == 0 {
                            if i == 0 {
                                "[ "
                            } else {
                                "  "
                            }
                        } else {
                            " "
                        },
                        s,
                        if j == self.num_columns - 1 {
                            if i == self.num_rows - 1 {
                                " ]"
                            } else {
                                "\n"
                            }
                        } else {
                            " "
                        }
                    )?;
                }
            }
        } else {
            writeln!(f, "mat is too big to print")?;
        }
        Ok(())
    }
}

impl Add for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn add(self, rhs: Self) -> Self::Output {
        assert_eq!(
            self.num_rows, rhs.num_rows,
            "num rows must match when performing addition"
        );
        assert_eq!(
            self.num_columns, rhs.num_columns,
            "num columns must match when performing addition"
        );

        Mat2D::from_rows(
            (0..self.num_rows)
                .into_par_iter()
                .flat_map(move |i| {
                    (0..self.num_columns)
                        .into_par_iter()
                        .map(move |j| self[(i, j)] + rhs[(i, j)])
                })
                .collect(),
            self.num_rows,
            self.num_columns,
        )
    }
}
impl Add for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn add(self, rhs: Self) -> Self::Output {
        &self + &rhs
    }
}
impl Add<&Mat2D<f32>> for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn add(self, rhs: &Mat2D<f32>) -> Self::Output {
        &self + rhs
    }
}
impl Add<Mat2D<f32>> for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn add(self, rhs: Mat2D<f32>) -> Self::Output {
        self + &rhs
    }
}

impl Sub for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn sub(self, rhs: Self) -> Self::Output {
        assert_eq!(
            self.num_rows, rhs.num_rows,
            "num rows must match when performing addition"
        );
        assert_eq!(
            self.num_columns, rhs.num_columns,
            "num columns must match when performing addition"
        );

        Mat2D::from_rows(
            (0..self.num_rows)
                .into_par_iter()
                .flat_map(move |i| {
                    (0..self.num_columns)
                        .into_par_iter()
                        .map(move |j| self[(i, j)] - rhs[(i, j)])
                })
                .collect(),
            self.num_rows,
            self.num_columns,
        )
    }
}
impl Sub for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn sub(self, rhs: Self) -> Self::Output {
        &self - &rhs
    }
}
impl Sub<&Mat2D<f32>> for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn sub(self, rhs: &Mat2D<f32>) -> Self::Output {
        &self - rhs
    }
}
impl Sub<Mat2D<f32>> for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn sub(self, rhs: Mat2D<f32>) -> Self::Output {
        self - &rhs
    }
}

impl Mul for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn mul(self, rhs: Self) -> Self::Output {
        assert_eq!(
            self.num_columns, rhs.num_rows,
            "rows of the first mat must match columns of the second mat when performing multiplication"
        );

        Mat2D::from_rows(
            (0..self.num_rows)
                .into_par_iter()
                .flat_map(move |i| {
                    (0..rhs.num_columns).into_par_iter().map(move |j| {
                        (0..self.num_columns)
                            .map(|k| self[(i, k)] * rhs[(k, j)])
                            .sum::<f32>()
                    })
                })
                .collect(),
            self.num_rows,
            rhs.num_columns,
        )
    }
}
impl Mul for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn mul(self, rhs: Self) -> Self::Output {
        &self * &rhs
    }
}
impl Mul<&Mat2D<f32>> for Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn mul(self, rhs: &Mat2D<f32>) -> Self::Output {
        &self * rhs
    }
}
impl Mul<Mat2D<f32>> for &Mat2D<f32> {
    type Output = Mat2D<f32>;

    fn mul(self, rhs: Mat2D<f32>) -> Self::Output {
        self * &rhs
    }
}

#[cfg(test)]
mod test {

    use super::Mat2D;

    #[test]
    fn mat2d_filled_with() {
        let m = Mat2D::filled_with(0, 2, 3);

        assert_eq!(
            m,
            Mat2D {
                num_rows: 2,
                num_columns: 3,
                vec: vec![0, 0, 0, 0, 0, 0],
            }
        );
    }

    #[test]
    fn mat2d_from_rows() {
        let v = vec![1, 2, 3, 4, 5, 6];

        let m = Mat2D::from_rows(v.clone(), 2, 3);

        assert_eq!(
            m,
            Mat2D {
                num_rows: 2,
                num_columns: 3,
                vec: v,
            }
        );
    }

    #[test]
    #[should_panic]
    fn mat2d_from_rows_panic_cases() {
        let _ = Mat2D::from_rows(vec![1, 2, 3, 4], 2, 3);
        let _ = Mat2D::from_rows(vec![1, 2, 3, 4, 5, 6, 7], 2, 3);
    }

    #[test]
    fn mat2d_index() {
        let m = Mat2D::from_rows(vec![1, 2, 3, 4, 5, 6], 2, 3);

        assert_eq!(m[(0, 0)], 1);
        assert_eq!(m[(0, 2)], 3);
        assert_eq!(m[(1, 2)], 6);
    }

    #[test]
    fn mat2d_transpose() {
        let m1 = Mat2D::from_rows(vec![1, 2, 3, 4, 5, 6], 2, 3);
        let m2 = m1.transpose();
        // m1.transpose_in_place();

        // assert_eq!(m1[(0, 0)], 1);
        // assert_eq!(m1[(2, 0)], 3);
        // assert_eq!(m1[(2, 1)], 6);

        assert_eq!(m2[(0, 0)], 1);
        assert_eq!(m2[(2, 0)], 3);
        assert_eq!(m2[(2, 1)], 6);
    }

    #[test]
    fn mat2d_add() {
        let m1 = Mat2D::from_rows(vec![1., 2., 3., 4., 5., 6.], 2, 3);
        let m2 = Mat2D::from_rows(vec![6., 5., 4., 3., 2., 1.], 2, 3);

        assert_eq!(
            m1 + m2,
            Mat2D::from_rows(vec![7., 7., 7., 7., 7., 7.], 2, 3)
        );
    }

    #[test]
    #[should_panic]
    fn mat2d_add_panic_cases() {
        let m1 = Mat2D::from_rows(vec![1., 2., 3., 4., 5., 6.], 2, 3);
        let m2 = Mat2D::from_rows(vec![6., 5., 4., 3.], 2, 2);

        let _ = m1 + m2;
    }

    #[test]
    fn mat2d_element_wise_product() {
        let m1 = Mat2D::from_rows(vec![1., 2., 3., 4., 5., 6.], 2, 3);
        let m2 = Mat2D::from_rows(vec![6., 5., 4., 3., 2., 1.], 2, 3);

        assert_eq!(
            m1 + m2,
            Mat2D::from_rows(vec![7., 7., 7., 7., 7., 7.], 2, 3)
        );
    }

    #[test]
    fn mat2d_mul() {
        let m1 = Mat2D::from_rows([1., 2., 3., 4., 5., 6.].to_vec(), 2, 3);
        let m2 = Mat2D::from_rows([4., 3., 2.].to_vec(), 3, 1);

        assert_eq!(m1 * m2, Mat2D::from_rows([16., 43.].to_vec(), 2, 1));
    }
}
