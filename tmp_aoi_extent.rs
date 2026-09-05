fn main() {
    use purgatory_simulation::{aoi_policy_rects, WorldBounds, point_in_aabb};
    let bounds = WorldBounds::FOOTNOTE_TEST;
    let mut max_dx = 0.0f32;
    let mut max_dy = 0.0f32;
    let step = 0.5;
    let mut ox = bounds.min_x + 0.5;
    while ox <= bounds.max_x - 0.5 {
        let mut oy = bounds.min_y + 0.5;
        while oy <= bounds.max_y - 0.5 {
            let rects = aoi_policy_rects([ox, oy], bounds);
            max_dx = max_dx.max((rects.leave.max_x() - ox).abs()).max((rects.leave.min_x() - ox).abs());
            max_dy = max_dy.max((rects.leave.max_y() - oy).abs()).max((rects.leave.min_y() - oy).abs());
            oy += step;
        }
        ox += step;
    }
    println!("max leave extent from observer: dx={max_dx} dy={max_dy}");
}
