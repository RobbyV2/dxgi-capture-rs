use std::arch::x86_64::*;

// Adapted from https://github.com/mcy/vb64/blob/main/src/simd.rs

pub fn bgra_to_rgba(s: &mut [u8]) {
    if s.len() < 16 {
        return bgra_to_rgba_scalar(s);
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { bgra_to_rgba_simd_avx2(s) };
        }
        if is_x86_feature_detected!("ssse3") {
            return unsafe { bgra_to_rgba_simd_sse2(s) };
        }
    }

    bgra_to_rgba_scalar(s)
}

#[target_feature(enable = "ssse3")]
unsafe fn bgra_to_rgba_simd_sse2(buf: &mut [u8]) {
    unsafe {
        let mut chunks = buf.chunks_exact_mut(16);
        for chunk in &mut chunks {
            let data = _mm_loadu_si128(chunk.as_ptr() as *const _);
            let shuffled = _mm_shuffle_epi8(
                data,
                _mm_set_epi8(15, 12, 13, 14, 11, 8, 9, 10, 7, 4, 5, 6, 3, 0, 1, 2),
            );
            _mm_storeu_si128(chunk.as_mut_ptr() as *mut _, shuffled);
        }
        bgra_to_rgba_scalar(chunks.into_remainder());
    }
}

#[target_feature(enable = "avx2")]
unsafe fn bgra_to_rgba_simd_avx2(buf: &mut [u8]) {
    unsafe {
        let mut chunks = buf.chunks_exact_mut(32);
        for chunk in &mut chunks {
            let data = _mm256_loadu_si256(chunk.as_ptr() as *const _);
            let shuffled = _mm256_shuffle_epi8(
                data,
                _mm256_set_epi8(
                    31, 28, 29, 30, 27, 24, 25, 26, 23, 20, 21, 22, 19, 16, 17, 18, 15, 12, 13, 14,
                    11, 8, 9, 10, 7, 4, 5, 6, 3, 0, 1, 2,
                ),
            );
            _mm256_storeu_si256(chunk.as_mut_ptr() as *mut _, shuffled);
        }
        bgra_to_rgba_scalar(chunks.into_remainder());
    }
}

fn bgra_to_rgba_scalar(buf: &mut [u8]) {
    for chunk in buf.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
}
