use super::error::{HnswError, Result};

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn norm(v: &[f32]) -> f32 {
    dot(v, v).sqrt()
}

/// Scale `v` to unit length. Rejects empty, non-finite and zero vectors.
pub fn normalize(v: &mut [f32]) -> Result<()> {
    if v.is_empty() {
        return Err(HnswError::EmptyVector);
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err(HnswError::NonFinite);
    }
    let n = norm(v);
    if !n.is_finite() || n == 0.0 {
        return Err(HnswError::InvalidNorm);
    }
    for x in v.iter_mut() {
        *x /= n;
    }
    Ok(())
}

/// Cosine similarity for arbitrary (non-normalized) vectors, in [-1, 1].
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let (na, nb) = (norm(a), norm(b));
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    (dot(a, b) / (na * nb)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_gives_unit_length() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v).unwrap();
        assert!((norm(&v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_rejects_bad_input() {
        assert_eq!(normalize(&mut []), Err(HnswError::EmptyVector));
        assert_eq!(normalize(&mut [0.0, 0.0]), Err(HnswError::InvalidNorm));
        assert_eq!(normalize(&mut [f32::NAN, 1.0]), Err(HnswError::NonFinite));
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine_similarity(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }
}
