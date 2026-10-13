struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    eye: vec4<f32>,
    hole: vec4<f32>,
    hud: vec4<f32>,
    tonemap: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_fwd: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
}

fn lens_image(p: vec3<f32>, secondary: bool) -> vec3<f32> {
    let eye = globals.eye.xyz;
    let hole = globals.hole.xyz;
    let r_s = globals.hole.w;

    if (r_s <= 0.0) {
        return p;
    }

    let to_hole = hole - eye;
    let d_l = length(to_hole);

    if (d_l < 1e-4) {
        return p;
    }

    let axis = to_hole / d_l;

    let to_p = p - eye;
    let d_s = length(to_p);

    if (d_s < 1e-4) {
        return p;
    }

    let dir = to_p / d_s;
    let d_ls = d_s - d_l;

    if (d_ls <= 0.0) {
        return p;
    }

    let theta_e_sq = 2.0 * r_s * d_ls / (d_l * d_s);

    let cos_beta = clamp(dot(dir, axis), -1.0, 1.0);
    let beta = acos(cos_beta);

    let perp = dir - axis * cos_beta;
    let perp_len = length(perp);

    if (perp_len < 1e-6) {
        return p;
    }

    let root = sqrt(beta * beta + 4.0 * theta_e_sq);
    let unit_perp = perp / perp_len;

    if (secondary) {
        let theta = 0.5 * (root - beta);
        return eye + (axis * cos(theta) - unit_perp * sin(theta)) * d_s;
    }

    let theta = 0.5 * (beta + root);
    return eye + (axis * cos(theta) + unit_perp * sin(theta)) * d_s;
}

fn lens(p: vec3<f32>) -> vec3<f32> {
    return lens_image(p, false);
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = globals.view_proj * vec4<f32>(model.position, 1.0);
    out.normal = model.normal;

    return out;
}

@vertex
fn vs_lensed(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = globals.view_proj * vec4<f32>(lens(model.position), 1.0);
    out.normal = model.normal;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let to_light = normalize(-globals.light_dir.xyz);

    let diffuse = max(dot(normal, to_light), 0.0);
    let ambient = 0.06;

    let base = vec3<f32>(0.0, 0.0, 0.0);
    let lit = base * (ambient + diffuse * (1.0 - ambient));

    return vec4<f32>(lit, 1.0);
}

@fragment
fn fs_grid(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.35, 0.55, 1.0);
}

fn glyph(d: i32) -> u32 {
    switch d {
        case 0: { return 31599u; }
        case 1: { return 11415u; }
        case 2: { return 29671u; }
        case 3: { return 29647u; }
        case 4: { return 23497u; }
        case 5: { return 31183u; }
        case 6: { return 31215u; }
        case 7: { return 29257u; }
        case 8: { return 31727u; }
        case 9: { return 31695u; }
        default: { return 2u; }
    }
}

struct HudOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_hud(@builtin(vertex_index) index: u32) -> HudOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );

    let corner = corners[index];

    let aspect = max(globals.hud.y, 0.0001);
    let height = 0.11;
    let width = 2.6 * height / aspect;

    let right = 0.97;
    let top = 0.96;

    var out: HudOutput;
    out.clip_position = vec4<f32>(
        right - width + corner.x * width,
        top - corner.y * height,
        0.0,
        1.0,
    );
    out.uv = corner;

    return out;
}

@fragment
fn fs_hud(in: HudOutput) -> @location(0) vec4<f32> {
    let cell_f = in.uv.x * 4.0;
    let cell = i32(floor(cell_f));
    let local = fract(cell_f);

    if (local > 0.82) {
        discard;
    }

    let ms = clamp(globals.hud.x, 0.0, 99.9);

    var digit: i32;
    if (cell == 0) {
        digit = i32(ms / 10.0) % 10;
    } else if (cell == 1) {
        digit = i32(ms) % 10;
    } else if (cell == 2) {
        digit = 10;
    } else {
        digit = i32(ms * 10.0) % 10;
    }

    let col = i32(clamp(local / 0.82 * 3.0, 0.0, 2.999));
    let row = i32(clamp(in.uv.y * 5.0, 0.0, 4.999));

    let bit = (glyph(digit) >> u32(14 - (row * 3 + col))) & 1u;

    if (bit == 0u) {
        discard;
    }

    return vec4<f32>(1.0, 0.05, 0.05, 1.0);
}


struct ParticleInput {
    @location(0) position: vec3<f32>,
    @location(1) alive: f32,
    @location(2) velocity: vec3<f32>,
}

struct ParticleOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

fn particle_shade(position: vec3<f32>, velocity: vec3<f32>, dim: f32) -> vec3<f32> {
    let eye = globals.eye.xyz;
    let hole = globals.hole.xyz;
    let r_s = globals.hole.w;
    let c = max(globals.hud.z, 1e-4);

    let to_eye = normalize(eye - position);

    let beta_vec = velocity / c;
    let beta = min(length(beta_vec), 0.99);
    let beta_los = dot(beta_vec, to_eye);

    let gamma = 1.0 / sqrt(max(1.0 - beta * beta, 1e-4));

    let doppler = 1.0 / (gamma * max(1.0 - beta_los, 1e-3));

    let r = max(length(position - hole), r_s * 1.001);
    let redshift = sqrt(max(1.0 - r_s / r, 0.0));

    let shift = doppler * redshift;

    let brightness = clamp(pow(shift, 4.0), 0.0, 12.0);

    let warm = vec3<f32>(1.0, 0.42, 0.12);
    let cool = vec3<f32>(0.62, 0.78, 1.0);
    let tint = mix(warm, cool, clamp((shift - 0.7) * 1.6, 0.0, 1.0));

    return tint * brightness * dim;
}

@vertex
fn vs_particle(model: ParticleInput) -> ParticleOutput {
    var out: ParticleOutput;
    out.color = vec3<f32>(0.0);

    if (model.alive < 0.5) {
        out.clip_position = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }

    out.clip_position = globals.view_proj * vec4<f32>(lens(model.position), 1.0);
    out.color = particle_shade(model.position, model.velocity, 0.06);

    return out;
}

@vertex
fn vs_particle_secondary(model: ParticleInput) -> ParticleOutput {
    var out: ParticleOutput;
    out.color = vec3<f32>(0.0);

    if (model.alive < 0.5) {
        out.clip_position = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }

    out.clip_position = globals.view_proj * vec4<f32>(lens_image(model.position, true), 1.0);

    out.color = particle_shade(model.position, model.velocity, 0.02);

    return out;
}

@fragment
fn fs_particle_shaded(in: ParticleOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

@group(1) @binding(0) var hdr_texture: texture_2d<f32>;
@group(1) @binding(1) var hdr_sampler: sampler;

struct FullscreenOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullscreenOutput {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    let corner = corners[index];

    var out: FullscreenOutput;
    out.clip_position = vec4<f32>(corner, 0.0, 1.0);
    out.uv = corner * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);

    return out;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;

    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_tonemap(in: FullscreenOutput) -> @location(0) vec4<f32> {
    var color = textureSample(hdr_texture, hdr_sampler, in.uv).rgb;

    color += textureSample(bloom_texture, bloom_sampler, in.uv).rgb * globals.tonemap.w;

    color = aces(color * globals.tonemap.x);

    if (globals.tonemap.y < 0.5) {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }

    return vec4<f32>(color, 1.0);
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn starfield(dir: vec3<f32>) -> vec3<f32> {
    let p = dir * 140.0;
    let cell = floor(p);
    let offset = fract(p) - 0.5;

    let h = hash31(cell);

    if (h < 0.986) {
        return vec3<f32>(0.0);
    }

    let magnitude = pow((h - 0.986) / 0.014, 2.0);
    let falloff = exp(-dot(offset, offset) * 34.0);
    let tint = mix(
        vec3<f32>(0.70, 0.80, 1.0),
        vec3<f32>(1.0, 0.88, 0.68),
        hash31(cell + vec3<f32>(7.31)),
    );

    return tint * magnitude * falloff * 4.0;
}

// Kerr-Schild coordinates have the spin along +z, the world has it along +y.
fn to_kerr(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(v.z, v.x, v.y);
}

fn from_kerr(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(v.y, v.z, v.x);
}

// Boyer-Lindquist r of a Kerr-Schild point, from r^4 - (x^2 + y^2 + z^2 - a^2) r^2 - a^2 z^2 = 0
fn kerr_r(x: vec3<f32>, a: f32) -> f32 {
    let a2 = a * a;
    let k = dot(x, x) - a2;
    return sqrt(max(0.5 * (k + sqrt(k * k + 4.0 * a2 * x.z * x.z)), 1e-8));
}

// g^uv = eta^uv - f l^u l^v, returns (l.xyz, f)
fn kerr_lf(x: vec3<f32>, a: f32, m: f32) -> vec4<f32> {
    let r = kerr_r(x, a);
    let r2 = r * r;
    let w = r2 + a * a;
    let l = vec3<f32>((r * x.x + a * x.y) / w, (r * x.y - a * x.x) / w, x.z / r);
    let f = 2.0 * m * r2 * r / (r2 * r2 + a * a * x.z * x.z);
    return vec4<f32>(l, f);
}

struct KerrStep {
    dx: vec3<f32>,
    dq: vec3<f32>,
}

// Hamilton's equations for H = 1/2 g^uv p_u p_v with the spatial momentum q and
// p_t = -1, so l^u p_u = l.q + 1 and H = 1/2 (q.q - 1 - f (l.q + 1)^2).
// These coordinates are regular for light falling in, not for light coming out.
fn kerr_step(x: vec3<f32>, q: vec3<f32>, a: f32, m: f32) -> KerrStep {
    let a2 = a * a;
    let k = dot(x, x) - a2;
    let d = max(sqrt(k * k + 4.0 * a2 * x.z * x.z), 1e-6);
    let r = sqrt(max(0.5 * (k + d), 1e-8));
    let r2 = r * r;

    let grad_r = vec3<f32>(r * x.x, r * x.y, r * x.z + a2 * x.z / r) / d;

    let n = r2 * r2 + a2 * x.z * x.z;
    let f = 2.0 * m * r2 * r / n;
    let grad_f = ((3.0 * a2 * x.z * x.z - r2 * r2) * grad_r - vec3<f32>(0.0, 0.0, 2.0 * a2 * x.z * r))
        * (2.0 * m * r2 / (n * n));

    let w = r2 + a2;
    let l = vec3<f32>((r * x.x + a * x.y) / w, (r * x.y - a * x.x) / w, x.z / r);
    let lq_xy = l.x * q.x + l.y * q.y;

    let grad_lq = (grad_r * (x.x * q.x + x.y * q.y) + vec3<f32>(r * q.x - a * q.y, a * q.x + r * q.y, 0.0)) / w
        - grad_r * (2.0 * r * lq_xy / w)
        + vec3<f32>(0.0, 0.0, q.z / r)
        - grad_r * (x.z * q.z / r2);

    let s = dot(l, q) + 1.0;

    var out: KerrStep;
    out.dx = q - l * (f * s);
    out.dq = grad_f * (0.5 * s * s) + grad_lq * (f * s);
    return out;
}

fn march(origin: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {
    let r_s = max(globals.hole.w, 1e-4);
    let m = 0.5 * r_s;
    // A ray traced back from the camera is light coming out of the hole, which
    // blows up at the horizon in these coordinates. Time reversed, it is light
    // falling into a hole spinning the other way, which does not.
    let a = -clamp(globals.hud.w, 0.0, 0.999) * m;
    let horizon = m + sqrt(max(m * m - a * a, 0.0));

    var x = to_kerr(origin - globals.hole.xyz);
    let d = to_kerr(normalize(direction));

    // pick q so the ray leaves the camera along d and is null
    let lf = kerr_lf(x, a, m);
    let f0 = min(lf.w, 0.9);
    let cos_l = dot(lf.xyz, d);
    let speed = inverseSqrt(1.0 - f0 * (1.0 - cos_l * cos_l));
    var q = d * speed + lf.xyz * (f0 * (speed * cos_l + 1.0) / (1.0 - f0));

    var vel = d;

    for (var step = 0; step < 220; step = step + 1) {
        let r = kerr_r(x, a);

        if (r <= horizon) {
            return vec3<f32>(0.0);
        }

        if (r > 400.0) {
            break;
        }

        let k1 = kerr_step(x, q, a, m);
        vel = k1.dx;

        let dl = clamp(0.14 * (r - horizon), 0.015, 3.0) / max(length(k1.dx), 1e-3);

        let k2 = kerr_step(x + k1.dx * (dl * 0.5), q + k1.dq * (dl * 0.5), a, m);
        let k3 = kerr_step(x + k2.dx * (dl * 0.5), q + k2.dq * (dl * 0.5), a, m);
        let k4 = kerr_step(x + k3.dx * dl, q + k3.dq * dl, a, m);

        x += (k1.dx + k2.dx * 2.0 + k3.dx * 2.0 + k4.dx) * (dl / 6.0);
        q += (k1.dq + k2.dq * 2.0 + k3.dq * 2.0 + k4.dq) * (dl / 6.0);
    }

    return starfield(from_kerr(normalize(vel)));
}

@fragment
fn fs_raymarch(in: FullscreenOutput) -> @location(0) vec4<f32> {
    if (globals.tonemap.z < 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let ndc = in.uv * 2.0 - 1.0;
    let tan_half = globals.cam_fwd.w;
    let aspect = max(globals.hud.y, 1e-4);

    let direction = normalize(
        globals.cam_fwd.xyz
            + globals.cam_right.xyz * (ndc.x * tan_half * aspect)
            + globals.cam_up.xyz * (-ndc.y * tan_half)
    );

    return vec4<f32>(march(globals.eye.xyz, direction), 1.0);
}

@group(2) @binding(0) var bloom_texture: texture_2d<f32>;
@group(2) @binding(1) var bloom_sampler: sampler;

@fragment
fn fs_bright(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let color = textureSample(hdr_texture, hdr_sampler, in.uv).rgb;
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));

    let threshold = 0.7;
    let knee = 0.4;
    let soft = clamp((luma - threshold + knee) / max(2.0 * knee, 1e-4), 0.0, 1.0);
    let weight = max(soft * soft * (luma - threshold), 0.0) / max(luma, 1e-4);

    return vec4<f32>(color * weight, 1.0);
}

fn blur(uv: vec2<f32>, axis: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(hdr_texture, 0));
    let step = texel * axis;

    let o1 = 1.3846153846;
    let o2 = 3.2307692308;

    var sum = textureSample(hdr_texture, hdr_sampler, uv).rgb * 0.2270270270;

    sum += (textureSample(hdr_texture, hdr_sampler, uv + step * o1).rgb
        + textureSample(hdr_texture, hdr_sampler, uv - step * o1).rgb) * 0.3162162162;

    sum += (textureSample(hdr_texture, hdr_sampler, uv + step * o2).rgb
        + textureSample(hdr_texture, hdr_sampler, uv - step * o2).rgb) * 0.0702702703;

    return sum;
}

@fragment
fn fs_blur_h(in: FullscreenOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(blur(in.uv, vec2<f32>(1.0, 0.0)), 1.0);
}

@fragment
fn fs_blur_v(in: FullscreenOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(blur(in.uv, vec2<f32>(0.0, 1.0)), 1.0);
}
