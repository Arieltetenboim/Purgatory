// Generated from purgatory-skeleton Humanoid v0. Do not hand-edit.
window.PURGATORY_HUMANOID_V0_CONTRACT = {
  version: 1,
  rig: 'humanoid_v0',
  bones: {
    'root': { parent: null, t: [0.00, 0.00], r: 0.00 },
    'pelvis': { parent: 'root', t: [0.00, 0.42], r: 0.00 },
    'torso': { parent: 'pelvis', t: [0.00, 0.30], r: 0.00 },
    'head': { parent: 'torso', t: [0.07, 0.24], r: 0.00 },
    'upper_arm_front': { parent: 'torso', t: [-0.10, 0.14], r: 0.00 },
    'lower_arm_front': { parent: 'upper_arm_front', t: [0.00, -0.22], r: 0.00 },
    'hand_front': { parent: 'lower_arm_front', t: [0.00, -0.10], r: 0.00 },
    'upper_arm_back': { parent: 'torso', t: [0.08, 0.14], r: 0.00 },
    'lower_arm_back': { parent: 'upper_arm_back', t: [0.00, -0.22], r: 0.50 },
    'hand_back': { parent: 'lower_arm_back', t: [0.00, -0.10], r: 0.00 },
    'upper_leg_front': { parent: 'pelvis', t: [-0.06, -0.04], r: 0.00 },
    'lower_leg_front': { parent: 'upper_leg_front', t: [0.00, -0.20], r: 0.00 },
    'foot_front': { parent: 'lower_leg_front', t: [0.00, -0.18], r: 0.00 },
    'upper_leg_back': { parent: 'pelvis', t: [0.04, -0.04], r: 0.00 },
    'lower_leg_back': { parent: 'upper_leg_back', t: [0.00, -0.20], r: 0.00 },
    'foot_back': { parent: 'lower_leg_back', t: [0.00, -0.18], r: 0.00 },
  },
  template_v1: {
    width: 2048, height: 2048, pixels_per_unit: 512.00, cell_size: 512,
    cells: [
      { part: 'head', x: 0, y: 0, w: 512, h: 512, pivot: [256, 384] },
      { part: 'torso', x: 512, y: 0, w: 512, h: 512, pivot: [256, 384] },
      { part: 'upper_arm_back', x: 1024, y: 0, w: 512, h: 512, pivot: [256, 128] },
      { part: 'upper_arm_front', x: 1536, y: 0, w: 512, h: 512, pivot: [256, 128] },
      { part: 'lower_arm_back', x: 0, y: 512, w: 512, h: 512, pivot: [256, 128] },
      { part: 'lower_arm_front', x: 512, y: 512, w: 512, h: 512, pivot: [256, 128] },
      { part: 'hand_back', x: 1024, y: 512, w: 512, h: 512, pivot: [256, 128] },
      { part: 'hand_front', x: 1536, y: 512, w: 512, h: 512, pivot: [256, 128] },
      { part: 'upper_leg_back', x: 0, y: 1024, w: 512, h: 512, pivot: [256, 96] },
      { part: 'upper_leg_front', x: 512, y: 1024, w: 512, h: 512, pivot: [256, 96] },
      { part: 'lower_leg_back', x: 1024, y: 1024, w: 512, h: 512, pivot: [256, 96] },
      { part: 'lower_leg_front', x: 1536, y: 1024, w: 512, h: 512, pivot: [256, 96] },
      { part: 'foot_back', x: 0, y: 1536, w: 512, h: 512, pivot: [160, 128] },
      { part: 'foot_front', x: 512, y: 1536, w: 512, h: 512, pivot: [160, 128] },
    ]
  }
};
