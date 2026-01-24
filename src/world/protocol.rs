pub enum WorldCommit {
    PlayerMoved { fid: u32, x: i32, y: i32 },
    BiomeExplored,
}
