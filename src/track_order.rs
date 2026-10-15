pub(crate) fn insertion_slot(heights: &[f32], pointer: f32) -> usize {
    let mut top = 0.;
    for (index, height) in heights.iter().enumerate() {
        if pointer < top + height / 2. {
            return index;
        }
        top += height;
    }
    heights.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_boundary_matches_drop_order_for_resized_tracks() {
        let heights = [100., 200., 72.];
        for (pointer, slot) in [
            (-10., 0),
            (49., 0),
            (50., 1),
            (199., 1),
            (200., 2),
            (335., 2),
            (336., 3),
            (500., 3),
        ] {
            assert_eq!(insertion_slot(&heights, pointer), slot);
            for source in 0..heights.len() {
                let destination = slot - usize::from(slot > source);
                let mut tracks = vec![0, 1, 2];
                let moved = tracks.remove(source);
                tracks.insert(destination, moved);
                assert_eq!(tracks[destination], source);
                assert!(destination < heights.len());
            }
        }
    }
}
