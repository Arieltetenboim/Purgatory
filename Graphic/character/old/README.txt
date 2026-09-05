ORIGINAL 2D SIDE-SCROLLER PLAYER CHARACTER

Contents:
- player_master_left.png: high-resolution identity reference.
- player_sprite_sheet_5x6_left.png: full transparent sprite sheet.
- frames/: 30 fixed-cell PNG frames with Alpha transparency.

Animations (6 frames each):
- idle
- walk
- run
- jump (anticipation, takeoff, rise, apex/fall, landing)
- hurt_recovery

Implementation notes:
- All frames face left.
- Mirror horizontally in the game engine for right-facing movement.
- Keep the same bottom-center pivot/anchor for every frame.
- Suggested starting playback: idle 8 FPS, walk 10 FPS, run 12 FPS,
  jump driven by player state, hurt/recovery 10 FPS.
- FPS means Frames Per Second: the number of animation images shown per second.
- PNG means Portable Network Graphics; Alpha is the transparency channel.
