//! ECS demo: bouncing particles driven by the sillyecs archetype ECS.
//!
//! sillyecs generates the World/components/systems from `ecs.yaml` in `build.rs`; this file
//! includes the generated code and fills in the per-game pieces - component data, the
//! Physics system and a system factory; the toolkit's `ChannelQueue` is the command queue. The
//! generated `world.apply_system_phases()` runs the fixed-update phase from real-time deltas,
//! so `AppConfig::fixed_timestep` stays `None`, and the world's clock is reset after a gap
//! (focus loss, a long frame) so it doesn't fast-forward through it.

use game_toolkit::prelude::*;

// Component payloads referenced by the generated `*Component` wrappers (`PositionData` ...).
type PositionData = Vec2;
type VelocityData = Vec2;

// The generated ECS finds `PositionData` / `VelocityData` here, in its parent module.
sillyecs::include_ecs!(mod ecs);
use ecs::*;

// The demo issues no game-specific commands, so the user-command slot is uninhabited.
type UserCommand = std::convert::Infallible;
type Command = WorldCommand<UserCommand>;

impl<E, Q> WorldUserCommandHandler for MainWorld<E, Q>
where
    Q: WorldUserCommand<UserCommand = UserCommand>,
{
    fn handle_user_command(&mut self, command: UserCommand) {
        match command {}
    }
}

/// State for the Physics system: the world bounds particles bounce off.
#[derive(Debug, Default)]
pub struct PhysicsSystemData {
    bounds: [f32; 2],
}

/// Builds system instances for the world.
struct SystemFactory {
    bounds: [f32; 2],
}

impl CreateSystem<PhysicsSystem> for SystemFactory {
    fn create(&self) -> PhysicsSystem {
        PhysicsSystem::from(PhysicsSystemData {
            bounds: self.bounds,
        })
    }
}

impl ApplyPhysicsSystem for PhysicsSystem {
    type Error = std::convert::Infallible;

    fn apply_many(
        &mut self,
        positions: &mut [PositionComponent],
        velocities: &mut [VelocityComponent],
    ) {
        let dt = SystemPhase::FIXED_UPDATE_SECS;
        let [bw, bh] = self.bounds;
        for (pos, vel) in positions.iter_mut().zip(velocities.iter_mut()) {
            pos.x += vel.x * dt;
            pos.y += vel.y * dt;
            if pos.x < 0.0 {
                pos.x = 0.0;
                vel.x = vel.x.abs();
            } else if pos.x > bw {
                pos.x = bw;
                vel.x = -vel.x.abs();
            }
            if pos.y < 0.0 {
                pos.y = 0.0;
                vel.y = vel.y.abs();
            } else if pos.y > bh {
                pos.y = bh;
                vel.y = -vel.y.abs();
            }
        }
    }
}

/// Longer frames than this reset the world's clock instead of catching up. Also passed as
/// `AppConfig::max_frame_time`, so the toolkit and the ECS agree on what a long frame is.
const MAX_FRAME_TIME: std::time::Duration = std::time::Duration::from_millis(250);

struct EcsDemo {
    world: MainWorld<NoOpPhaseEvents, ChannelQueue<Command>>,
    entities: Vec<sillyecs::EntityId>,
}

impl Game for EcsDemo {
    fn init(ctx: &mut Context) -> Result<Self> {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let factory = SystemFactory { bounds: [w, h] };
        let mut world = MainWorld::new(&factory, ChannelQueue::new());

        let count = 48;
        let mut entities = Vec::with_capacity(count);
        for i in 0..count {
            let t = i as f32 / count as f32;
            let angle = t * std::f32::consts::TAU;
            let speed = 140.0 + 90.0 * (t * 7.0).fract();
            let pos = Vec2::new(w * 0.5, h * 0.5);
            let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);
            entities.push(
                world.spawn_particle_with(PositionComponent::new(pos), VelocityComponent::new(vel)),
            );
        }
        Ok(Self { world, entities })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        // sillyecs times its phases itself, so the toolkit's `max_frame_time` cap doesn't
        // reach it. After a long frame (window dragged, breakpoint), skip the gap rather than
        // replaying every missed fixed step.
        if ctx.time.delta > MAX_FRAME_TIME {
            self.world.reset_frame_context();
        }
        // Runs the FixedUpdate phase as many times as the elapsed real time allows.
        self.world.apply_system_phases();
    }

    fn event(&mut self, _ctx: &mut Context, event: &GameEvent) {
        // Same after time in the background.
        if let GameEvent::FocusChanged(true) = event {
            self.world.reset_frame_context();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let n = self.entities.len() as f32;
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.05, 0.06, 0.09, 1.0]);
        for (i, &id) in self.entities.iter().enumerate() {
            if let Some(e) = self.world.get_particle_entity(id) {
                let hue = i as f32 / n;
                let color = [0.4 + 0.5 * hue, 0.75, 1.0 - 0.5 * hue, 1.0];
                p.circle([e.position.x, e.position.y], 7.0, color);
            }
        }
        p.text(
            [16.0, 16.0],
            "11_ecs: sillyecs particles (Esc to quit)",
            22.0,
            [0.9, 0.9, 0.95, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<EcsDemo>(AppConfig {
        title: "11_ecs".into(),
        width: 800,
        height: 600,
        // The same cap the world's clock reset uses in `update`.
        max_frame_time: MAX_FRAME_TIME,
        ..Default::default()
    })
}
