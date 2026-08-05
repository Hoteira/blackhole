struct GpuParticle {
    position: vec3<f32>,
    alive: f32,
    velocity: vec3<f32>,
    _pad: f32,
}

struct SimParams {
    hole: vec4<f32>,
    config: vec4<f32>,
    tuning: vec4<f32>,
}

@group(0) @binding(0) var<storage, read_write> particles: array<GpuParticle>;
@group(0) @binding(1) var<uniform> sim: SimParams;

fn particle_accel(r_vec: vec3<f32>, v: vec3<f32>, m: f32, c: f32) -> vec3<f32> {
    let r_sq = dot(r_vec, r_vec);
    let r = sqrt(r_sq);

    if (r < 1e-4 || m <= 0.0) {
        return vec3<f32>(0.0);
    }

    let h = length(cross(r_vec, v));

    let newtonian = m * c * c / (r_sq * r);
    let relativistic = 3.0 * m * h * h / (r_sq * r_sq * r);

    return r_vec * -(newtonian + relativistic);
}

@compute @workgroup_size(64)
fn cs_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;

    if (index >= arrayLength(&particles)) {
        return;
    }

    var particle = particles[index];

    if (particle.alive < 0.5) {
        return;
    }

    let hole = sim.hole.xyz;
    let r_s = sim.hole.w;
    let m = 0.5 * r_s;

    let dt = sim.config.x;
    let half_extent = sim.config.y;
    let despawn = sim.config.z;
    let accretion_radius = sim.config.w;

    let radial_damping = sim.tuning.x;
    let tangential_drag = sim.tuning.y;
    let c = sim.tuning.z;

    let radius = length(particle.position - hole);
    var substeps = 2;
    if (radius < 3.0 * r_s) {
        substeps = 8;
    }
    let h = dt / f32(substeps);

    for (var step = 0; step < substeps; step = step + 1) {
        var p = particle.position - hole;
        var v = particle.velocity;

        let k1v = particle_accel(p, v, m, c);
        let k2v = particle_accel(p + v * (h * 0.5), v + k1v * (h * 0.5), m, c);
        let k3v = particle_accel(p + (v + k1v * (h * 0.5)) * (h * 0.5), v + k2v * (h * 0.5), m, c);
        let k4v = particle_accel(p + (v + k2v * (h * 0.5)) * h, v + k3v * h, m, c);

        let k1p = v;
        let k2p = v + k1v * (h * 0.5);
        let k3p = v + k2v * (h * 0.5);
        let k4p = v + k3v * h;

        p += (k1p + k2p * 2.0 + k3p * 2.0 + k4p) * (h / 6.0);
        v += (k1v + k2v * 2.0 + k3v * 2.0 + k4v) * (h / 6.0);

        let r_now = length(p);

        if (r_now > 1e-4 && r_now < accretion_radius) {
            let radial = p / r_now;
            let v_radial = radial * dot(v, radial);
            let v_tangential = v - v_radial;

            v -= v_radial * min(radial_damping * h, 1.0);
            v -= v_tangential * min(tangential_drag * h, 1.0);
        }

        let speed = length(v);
        if (speed > c) {
            v = v / speed * c;
        }

        particle.position = hole + p;
        particle.velocity = v;

        let r = length(p);
        let out_of_bounds = abs(particle.position.x) > half_extent
            || abs(particle.position.z) > half_extent;

        if (r <= r_s || out_of_bounds || r > despawn) {
            particle.alive = 0.0;
            break;
        }
    }

    particles[index] = particle;
}
