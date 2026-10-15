//! Skeleton-driven, shaded people: ball handlers and runners in side view, runners seen from
//! the front/back for the end-on camera, and seated bench/table figures.
//!
//! Limbs are rounded capsules posed by forward kinematics (run cycle) or two-bone IK (reaching
//! the ball), shaded with 4-tone ramps from a top-left light and finished with a selective
//! outline, so the result reads as hand-drawn sprites rather than flat boxes.

use core::f32::consts::PI;

use crate::palette::{Ramp, BLACK, CLOTH, HAIRS, RED, SKINS, STEEL, SUIT};
use crate::sprite::Sprite;

#[derive(Clone, Copy)]
pub struct Look {
    pub skin: usize,
    pub hair: usize,
    pub top: Ramp,
    pub trim: Ramp,
    pub shorts: Ramp,
    pub number: u8,
    /// Referee shirt: horizontal black stripes on white.
    pub stripes: bool,
    /// Suit: long trousers, jacket, shirt and tie (coaches, officials).
    pub suit: bool,
}

#[derive(Clone, Copy)]
pub struct Pose {
    /// Run-cycle phase in radians.
    pub phase: f32,
    pub moving: bool,
    pub jump: bool,
    /// Where the ball is, relative to the feet and already mirrored for facing (px, y down).
    pub ball_hand: Option<(f32, f32)>,
}

pub const STILL: Pose = Pose {
    phase: 0.0,
    moving: false,
    jump: false,
    ball_hand: None,
};

const L_THIGH: f32 = 8.2;
const L_SHIN: f32 = 7.2;
const L_UPPER: f32 = 5.4;
const L_FORE: f32 = 5.0;

fn shoe_ramp(black: bool) -> Ramp {
    if black {
        SUIT
    } else {
        CLOTH
    }
}

/// Elbow position for a two-bone arm reaching `target` (picks the elbow-down solution).
fn ik_elbow(shoulder: (f32, f32), target: (f32, f32)) -> ((f32, f32), (f32, f32)) {
    let (dx, dy) = (target.0 - shoulder.0, target.1 - shoulder.1);
    let d = (dx * dx + dy * dy)
        .sqrt()
        .clamp(0.5, L_UPPER + L_FORE - 0.05);
    let base = dy.atan2(dx);
    let a = ((L_UPPER * L_UPPER + d * d - L_FORE * L_FORE) / (2.0 * L_UPPER * d))
        .clamp(-1.0, 1.0)
        .acos();
    let cands = [base + a, base - a].map(|t| {
        (
            shoulder.0 + L_UPPER * t.cos(),
            shoulder.1 + L_UPPER * t.sin(),
        )
    });
    let elbow = if cands[0].1 > cands[1].1 {
        cands[0]
    } else {
        cands[1]
    };
    // The hand lands on the (clamped) target direction.
    let (ex, ey) = (target.0 - elbow.0, target.1 - elbow.1);
    let el = (ex * ex + ey * ey).sqrt().max(0.001);
    (
        elbow,
        (elbow.0 + ex / el * L_FORE, elbow.1 + ey / el * L_FORE),
    )
}

/// Thigh and shin angles (from straight down, forward positive) for leg `leg` (0 = near).
fn leg_angles(pose: &Pose, leg: usize) -> (f32, f32) {
    if pose.jump {
        let a = 0.3 + 0.2 * leg as f32;
        (a, a - 1.1)
    } else if !pose.moving {
        let a = if leg == 0 { 0.1 } else { -0.1 };
        (a, a - 0.05)
    } else {
        let p = pose.phase + if leg == 0 { 0.0 } else { PI };
        let a = 0.8 * p.sin();
        (a, a - (0.2 + p.cos().max(0.0)))
    }
}

fn arm_angles(pose: &Pose, arm: usize) -> (f32, f32) {
    if !pose.moving {
        let a = if arm == 0 { 0.1 } else { -0.05 };
        (a, a + 0.35)
    } else {
        let p = pose.phase + if arm == 0 { PI } else { 0.0 };
        let a = -0.85 * p.sin();
        (a, a + 1.1 + 0.3 * p.cos().max(0.0))
    }
}

fn hair_ramp(look: &Look) -> Ramp {
    HAIRS[look.hair.min(HAIRS.len() - 1)]
}

/// A 34 px tall person seen from the side, facing `f` (+1 right, -1 left). Origin = feet.
pub fn side_person(look: &Look, f: i32, pose: &Pose, black_shoes: bool) -> Sprite {
    let mut s = Sprite::new(60, 64, 30, 56);
    let ff = f as f32;
    let skin = SKINS[look.skin];
    let leg_ramp = if look.suit { look.shorts } else { skin };
    let lean = if pose.moving { 1.6 } else { 0.0 };

    // Pose the legs first: the lowest ankle fixes where the ground is.
    let legs = [leg_angles(pose, 0), leg_angles(pose, 1)];
    let ankle_dy: Vec<f32> = legs
        .iter()
        .map(|&(a, b)| L_THIGH * a.cos() + L_SHIN * b.cos())
        .collect();
    let hip_y = -2.5 - ankle_dy.iter().cloned().fold(f32::MIN, f32::max);
    let hip = (0.0, hip_y);
    let shoulder_y = hip_y - 9.0;

    let draw_leg = |s: &mut Sprite, (a1, a2): (f32, f32), bias: f32| {
        let knee = (hip.0 + ff * L_THIGH * a1.sin(), hip.1 + L_THIGH * a1.cos());
        let ankle = (knee.0 + ff * L_SHIN * a2.sin(), knee.1 + L_SHIN * a2.cos());
        s.capsule(hip.0, hip.1, knee.0, knee.1, 2.4, bias, &leg_ramp);
        s.capsule(knee.0, knee.1, ankle.0, ankle.1, 1.9, bias, &leg_ramp);
        if !look.suit {
            let mid = ((knee.0 + ankle.0) / 2.0, (knee.1 + ankle.1) / 2.0);
            s.capsule(mid.0, mid.1, ankle.0, ankle.1, 2.0, bias, &CLOTH);
        }
        s.ellipse(
            ankle.0 + ff * 1.6,
            ankle.1 + 1.3,
            3.4,
            1.8,
            bias,
            &shoe_ramp(black_shoes),
        );
    };

    // Far arm, far leg, shorts, near leg, torso, near arm, head.
    let far_arm = {
        let (a1, a2) = arm_angles(pose, 1);
        let sh = (lean - ff * 0.8, shoulder_y + 0.5);
        let el = (sh.0 + ff * L_UPPER * a1.sin(), sh.1 + L_UPPER * a1.cos());
        let hand = (el.0 + ff * L_FORE * a2.sin(), el.1 + L_FORE * a2.cos());
        (sh, el, hand)
    };
    let sleeve = if look.stripes || look.suit {
        look.top
    } else {
        skin
    };
    {
        let (sh, el, hand) = far_arm;
        if pose.jump {
            let (el, hand) = ik_elbow(sh, (sh.0 + ff * 0.5, sh.1 - 8.5));
            s.capsule(sh.0, sh.1, el.0, el.1, 1.5, -0.5, &sleeve);
            s.capsule(el.0, el.1, hand.0, hand.1, 1.3, -0.5, &skin);
            s.ellipse(hand.0, hand.1, 1.4, 1.4, -0.5, &skin);
        } else {
            s.capsule(sh.0, sh.1, el.0, el.1, 1.5, -0.5, &sleeve);
            s.capsule(el.0, el.1, hand.0, hand.1, 1.3, -0.5, &skin);
            s.ellipse(hand.0, hand.1, 1.4, 1.4, -0.5, &skin);
        }
    }
    draw_leg(&mut s, legs[1], -0.5);

    // Shorts (or trousers' top) with a trimmed hem.
    s.ellipse(hip.0 + lean * 0.3, hip_y + 0.5, 4.9, 4.5, 0.0, &look.shorts);
    if !look.suit {
        s.capsule(
            hip.0 - 4.0,
            hip_y + 4.2,
            hip.0 + 4.0,
            hip_y + 4.2,
            0.7,
            0.0,
            &look.trim,
        );
    }
    draw_leg(&mut s, legs[0], 0.0);

    // Torso.
    let (tx, ty) = (lean * 0.7, hip_y - 4.4);
    s.ellipse(tx, hip_y - 1.5, 4.2, 3.2, 0.0, &look.top);
    s.ellipse(tx, ty, 4.7, 5.8, 0.0, &look.top);
    if look.stripes {
        s.recolour_rows(
            (tx - 6.0) as i32,
            (ty - 6.0) as i32,
            12,
            13,
            2,
            &CLOTH,
            SUIT[0],
        );
    } else if look.suit {
        s.ellipse(tx + ff * 1.4, ty - 2.5, 1.7, 2.6, 0.0, &CLOTH);
        s.capsule(
            tx + ff * 1.4,
            ty - 3.5,
            tx + ff * 1.4,
            ty + 1.5,
            0.8,
            0.0,
            &RED,
        );
    } else {
        s.capsule(tx - 4.0, ty - 3.0, tx - 3.6, ty + 5.0, 0.7, 0.0, &look.trim);
        s.capsule(tx + 4.0, ty - 3.0, tx + 3.6, ty + 5.0, 0.7, 0.0, &look.trim);
        if look.number > 0 {
            let n = look.number.to_string();
            s.text(
                tx as i32 - Sprite::text_width(&n) / 2,
                ty as i32 - 2,
                &n,
                look.trim[2],
            );
        }
    }

    // Neck and head.
    let (hx, hy) = (lean + ff * 0.8, shoulder_y - 3.4);
    s.capsule(
        lean,
        shoulder_y - 1.0,
        lean,
        shoulder_y - 3.0,
        1.5,
        0.0,
        &skin,
    );
    s.ellipse(hx, hy, 3.2, 3.6, 0.0, &skin);
    s.ellipse(hx - ff * 0.9, hy - 1.3, 3.3, 2.8, 0.0, &hair_ramp(look));
    s.put(
        (hx + ff * 3.4).floor() as i32,
        (hy + 0.8).floor() as i32,
        skin[2],
    );
    s.put(
        (hx + ff * 1.4).floor() as i32,
        (hy + 0.2).floor() as i32,
        BLACK,
    );

    // Near arm: reaches for the ball when dribbling, up when shooting.
    {
        let sh = (lean + ff * 0.4, shoulder_y + 0.5);
        let (el, hand) = if let Some(target) = pose.ball_hand {
            ik_elbow(sh, target)
        } else if pose.jump {
            ik_elbow(sh, (sh.0 + ff * 3.0, sh.1 - 10.0))
        } else {
            let (a1, a2) = arm_angles(pose, 0);
            let el = (sh.0 + ff * L_UPPER * a1.sin(), sh.1 + L_UPPER * a1.cos());
            (
                el,
                (el.0 + ff * L_FORE * a2.sin(), el.1 + L_FORE * a2.cos()),
            )
        };
        s.capsule(sh.0, sh.1, el.0, el.1, 1.6, 0.0, &sleeve);
        s.capsule(el.0, el.1, hand.0, hand.1, 1.4, 0.0, &skin);
        s.ellipse(hand.0, hand.1, 1.5, 1.5, 0.0, &skin);
    }
    s.outline();
    s
}

/// A ~27 px tall person seen from behind (`back`) or the front, for the end-on camera.
pub fn end_person(look: &Look, back: bool, pose: &Pose, black_shoes: bool) -> Sprite {
    let mut s = Sprite::new(48, 56, 24, 46);
    let skin = SKINS[look.skin];
    let leg_ramp = if look.suit { look.shorts } else { skin };
    let hip_y = -13.5;
    let lifts = if pose.jump {
        [2.5, 2.5]
    } else if pose.moving {
        [
            pose.phase.sin().max(0.0) * 3.5,
            (-pose.phase.sin()).max(0.0) * 3.5,
        ]
    } else {
        [0.0, 0.0]
    };
    for (i, x) in [-2.2_f32, 2.2].iter().enumerate() {
        let lift = lifts[i];
        let knee_y = hip_y + 6.0 - lift * 0.5;
        let ankle_y = -2.4 - lift;
        s.capsule(*x, hip_y, *x, knee_y, 2.3, 0.0, &leg_ramp);
        s.capsule(*x, knee_y, *x, ankle_y, 1.9, 0.0, &leg_ramp);
        if !look.suit {
            s.capsule(
                *x,
                (knee_y + ankle_y) / 2.0 + 1.0,
                *x,
                ankle_y,
                2.0,
                0.0,
                &CLOTH,
            );
        }
        s.ellipse(*x, ankle_y + 1.3, 2.7, 1.7, 0.0, &shoe_ramp(black_shoes));
    }
    s.ellipse(0.0, hip_y + 0.5, 5.4, 4.4, 0.0, &look.shorts);
    if !look.suit {
        s.capsule(-4.8, hip_y + 4.2, 4.8, hip_y + 4.2, 0.7, 0.0, &look.trim);
    }
    let ty = hip_y - 4.8;
    // Arms hang at the sides (behind the torso edge so the shoulders overlap them).
    let swing = if pose.moving {
        pose.phase.sin() * 1.5
    } else {
        0.0
    };
    let sleeve = if look.stripes || look.suit {
        look.top
    } else {
        skin
    };
    for (i, sx) in [-5.0_f32, 5.0].iter().enumerate() {
        let side = if i == 0 { -1.0 } else { 1.0 };
        let sh = (*sx, ty - 3.4);
        let (el, hand) = if pose.jump {
            ((sx + side * 0.6, sh.1 - 4.5), (sx * 0.7, sh.1 - 9.5))
        } else if pose.ball_hand.is_some() && i == 1 {
            ((sx + 1.8, sh.1 + 4.0), (sx + 3.4, sh.1 + 6.0))
        } else {
            (
                (sx + side * 0.8, sh.1 + 4.5 + swing * side),
                (sx + side * 1.2, sh.1 + 9.0 + swing * side),
            )
        };
        s.capsule(sh.0, sh.1, el.0, el.1, 1.6, 0.0, &sleeve);
        s.capsule(el.0, el.1, hand.0, hand.1, 1.4, 0.0, &skin);
        s.ellipse(hand.0, hand.1, 1.5, 1.5, 0.0, &skin);
    }
    s.ellipse(0.0, hip_y - 1.5, 4.3, 3.2, 0.0, &look.top);
    s.ellipse(0.0, ty, 5.1, 5.6, 0.0, &look.top);
    if look.stripes {
        s.recolour_rows(-6, (ty - 6.0) as i32, 12, 13, 2, &CLOTH, SUIT[0]);
    } else if look.suit {
        if !back {
            s.ellipse(0.0, ty - 2.5, 1.8, 2.6, 0.0, &CLOTH);
            s.capsule(0.0, ty - 3.5, 0.0, ty + 1.5, 0.8, 0.0, &RED);
        }
    } else {
        s.capsule(-4.5, ty - 3.0, -4.2, ty + 5.0, 0.7, 0.0, &look.trim);
        s.capsule(4.5, ty - 3.0, 4.2, ty + 5.0, 0.7, 0.0, &look.trim);
        if look.number > 0 {
            let n = look.number.to_string();
            s.text(-Sprite::text_width(&n) / 2, ty as i32 - 2, &n, look.trim[2]);
        }
    }
    let hy = ty - 8.8;
    s.capsule(0.0, ty - 5.0, 0.0, ty - 6.8, 1.6, 0.0, &skin);
    s.ellipse(0.0, hy, 3.3, 3.6, 0.0, &skin);
    if back {
        s.ellipse(0.0, hy - 0.6, 3.4, 3.2, 0.0, &hair_ramp(look));
    } else {
        s.ellipse(0.0, hy - 1.6, 3.4, 2.3, 0.0, &hair_ramp(look));
        s.put(-2, (hy + 0.3) as i32, BLACK);
        s.put(1, (hy + 0.3) as i32, BLACK);
        s.put(0, (hy + 2.2) as i32, skin[1]);
    }
    s.outline();
    s
}

/// A seated figure (bench player, coach, official) seen from the front, ~24 px tall.
pub fn seated_person(jacket: Ramp, skin: usize, hair: usize, suit: bool) -> Sprite {
    let mut s = Sprite::new(36, 40, 18, 36);
    let skin = SKINS[skin];
    let leg_ramp = if suit { SUIT } else { jacket };
    s.rect(-6, -26, 13, 17, STEEL[1]); // folding chair back
    s.rect(-6, -26, 13, 1, STEEL[3]);
    for x in [-2.4_f32, 2.4] {
        s.capsule(x, -8.0, x, -4.5, 2.3, 0.0, &leg_ramp);
        s.capsule(x, -4.5, x, -2.0, 1.9, 0.0, &leg_ramp);
        s.ellipse(x, -0.8, 2.7, 1.6, 0.0, &CLOTH);
    }
    s.ellipse(0.0, -14.0, 5.2, 5.4, 0.0, &jacket);
    if suit {
        s.ellipse(0.0, -16.5, 1.9, 2.8, 0.0, &CLOTH);
        s.capsule(0.0, -17.5, 0.0, -12.5, 0.8, 0.0, &RED);
    } else {
        s.capsule(0.0, -18.0, 0.0, -9.0, 0.5, 0.0, &CLOTH);
    }
    s.capsule(-5.0, -17.0, -5.6, -10.5, 1.6, 0.0, &jacket);
    s.capsule(5.0, -17.0, 5.6, -10.5, 1.6, 0.0, &jacket);
    s.capsule(0.0, -19.0, 0.0, -20.5, 1.5, 0.0, &skin);
    s.ellipse(0.0, -22.4, 3.1, 3.4, 0.0, &skin);
    s.ellipse(0.0, -23.8, 3.3, 2.3, 0.0, &HAIRS[hair.min(HAIRS.len() - 1)]);
    s.put(-2, -22, BLACK);
    s.put(1, -22, BLACK);
    s.outline();
    s
}
