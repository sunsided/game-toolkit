// One Game of Life generation: read `src`, write `dst`, count the live cells and paint the
// texture. WORKGROUP_SIZE_X/Y are defined by game_toolkit_compute::Kernel.
//
// A cell is 255 while alive. A dead cell keeps a fading trail: it drops to 200 when it dies
// and loses a little every generation down to 0.

struct Params {
    width: u32,
    height: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> live: atomic<u32>;
@group(0) @binding(4) var out_tex: texture_storage_2d<rgba8unorm, write>;

const ALIVE: u32 = 255u;
const JUST_DIED: u32 = 200u;
const FADE: u32 = 5u;

fn alive(x: i32, y: i32) -> u32 {
    // Wrap around the edges: the grid is a torus.
    let w = i32(params.width);
    let h = i32(params.height);
    let wx = (x + w) % w;
    let wy = (y + h) % h;
    return select(0u, 1u, src[u32(wy * w + wx)] == ALIVE);
}

@compute @workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    // The dispatch rounds up to whole workgroups; skip the threads past the edge.
    if id.x >= params.width || id.y >= params.height {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var neighbours = 0u;
    for (var dy = -1; dy <= 1; dy += 1) {
        for (var dx = -1; dx <= 1; dx += 1) {
            if dx != 0 || dy != 0 {
                neighbours += alive(x + dx, y + dy);
            }
        }
    }

    let i = id.y * params.width + id.x;
    let was = src[i];
    var next: u32;
    if (was == ALIVE && (neighbours == 2u || neighbours == 3u)) || (was != ALIVE && neighbours == 3u) {
        next = ALIVE;
    } else if was == ALIVE {
        next = JUST_DIED;
    } else {
        next = select(0u, was - FADE, was > FADE);
    }
    dst[i] = next;

    var color: vec4<f32>;
    if next == ALIVE {
        atomicAdd(&live, 1u);
        color = vec4(1.0, 0.92, 0.70, 1.0);
    } else {
        let trail = f32(next) / f32(JUST_DIED);
        let background = vec4(0.05, 0.07, 0.12, 1.0);
        let glow = vec4(0.10, 0.55, 0.65, 1.0);
        color = mix(background, glow, trail * trail);
    }
    textureStore(out_tex, vec2<u32>(id.xy), color);
}
