//! The scripted players: where each player the pad is not steering wants to go, and how fast.
//!
//! This is the seam between "what a player wants" and the physics in [`Game::step`], which turns
//! a target and a speed into acceleration, clamping, facing and animation. It only looks at the
//! game, like a human would. (The pad-level opponent that steers a team's *controlled* player
//! lives in [`crate::cpu`].)

use crate::game::{members, team_of, team_x, BallState, Game, OFFENSE_SPOTS, PLAYERS, TEAM_SIZE};

/// Run speed of the player chasing a loose ball, in feet per second.
const CHASE_SPEED: f32 = 20.0;
/// Run speed of offensive players walking to their spots.
const SPOT_SPEED: f32 = 15.0;
/// Run speed of defenders keeping between their man and the basket.
const GUARD_SPEED: f32 = 16.0;

/// Per team, the player who runs after the ball while it is loose.
pub(crate) fn chasers(game: &Game) -> [Option<usize>; 2] {
    if game.ball.state == BallState::Loose {
        [0, 1].map(|t| Some(game.nearest_in(members(t), game.ball.x, game.ball.y, None)))
    } else {
        [None; 2]
    }
}

/// Where player `i` (not the team's controlled one) heads: `(target_x, target_y, speed)`.
/// `chaser` is the team's entry from [`chasers`].
pub(crate) fn steer(game: &Game, i: usize, chaser: Option<usize>) -> (f32, f32, f32) {
    debug_assert!(i < PLAYERS);
    let team = team_of(i);
    if chaser == Some(i) {
        (game.ball.x, game.ball.y, CHASE_SPEED)
    } else if team == game.offense {
        let (x, y) = OFFENSE_SPOTS[i % TEAM_SIZE];
        (team_x(team, x), y, SPOT_SPEED)
    } else {
        let (x, y) = game.guard_spot(i);
        (x, y, GUARD_SPEED)
    }
}
