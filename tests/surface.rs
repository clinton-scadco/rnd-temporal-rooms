use temporal_rooms::machine::form::{kit::{Mat, MATS}, surface::{Surface, SIZE}};

#[test]
fn textures_are_deterministic_and_have_complete_mip_chains() {
    let mut unique = std::collections::BTreeSet::new();
    for mat in MATS {
        let s = Surface::new(mat);
        assert_eq!(s.levels, Surface::new(mat).levels, "{mat}: texture changed between builds");
        assert_eq!(s.levels.len(), 7);
        for (level, pixels) in s.levels.iter().enumerate() {
            assert_eq!(pixels.len(), (SIZE >> level).pow(2) * 2, "{mat}: incomplete mip level");
        }
        unique.insert(s.levels[0].clone());
        let min = s.levels[0].iter().step_by(2).min().unwrap();
        let max = s.levels[0].iter().step_by(2).max().unwrap();
        assert!(max - min > 10, "{mat}: a flat swatch rather than a texture");
    }
    // Five paint colours intentionally share one enamel finish.
    assert!(unique.len() >= 7, "different materials need different surface structure");
}

#[test]
fn textures_repeat_without_seams_and_filter_to_a_stable_distant_colour() {
    for mat in MATS {
        let s = Surface::new(mat);
        for n in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.6, 0.8, 0.0]] {
            for p in [[-0.37, 0.21, 0.41], [0.0; 3], [0.6, -0.5, 0.9]] {
                let a = s.sample(p, n, 0.001);
                let b = s.sample(p.map(|v| v + s.metres), n, 0.001);
                for k in 0..2 { assert!((a[k] - b[k]).abs() < 1e-4, "{mat}: texture seam"); }
                let far = s.sample(p, n, 100.0);
                let elsewhere = s.sample([12.4, -3.1, 4.2], n, 100.0);
                for k in 0..2 { assert!((far[k] - elsewhere[k]).abs() < 1e-5, "{mat}: distant grain aliases"); }
                assert!((0.7..=1.3).contains(&a[0]) && (-0.2..=0.2).contains(&a[1]));
            }
        }
    }
}

#[test]
fn brushed_steel_has_directional_grain() {
    let s = Surface::new(Mat::Steel);
    let pixels = &s.levels[0];
    let mut along = 0u64;
    let mut across = 0u64;
    for y in 0..SIZE - 1 {
        for x in 0..SIZE - 1 {
            let p = pixels[(y * SIZE + x) * 2] as i32;
            along += (p - pixels[(y * SIZE + x + 1) * 2] as i32).unsigned_abs() as u64;
            across += (p - pixels[((y + 1) * SIZE + x) * 2] as i32).unsigned_abs() as u64;
        }
    }
    assert!(across > along * 2, "steel should read as brushed, not random noise");
}
