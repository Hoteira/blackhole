# Blackhole

![screenshot](https://github.com/Hoteira/blackhole/blob/master/image.png)

A black hole toy shader / """simulation""" written to learn wgpu.
Nothing here is a serious simulation, but most of the numbers come from real formulas.
This is not an example of clean, well-documented code, just a weekend passion project.

## Running

```bash
cargo run --release
```

I'd not recommend debug mode as the framerate isn't great.

| Key | Action |
| --- | --- |
| Left drag | Orbit the camera |
| Scroll | Zoom |
| F | Fire a burst of matter at the hole, hold to keep firing (though the result looks better if you tap F for every single one of them) |
| Space | Toggle the grid lensing |
| V | Toggle vsync |
| R | Toggle the raymarched background |

The red number in the corner is the frame time in milliseconds.

## Units

Everything is measured in Schwarzschild radii. The horizon is exactly one world
unit, so the mass is not a free parameter: it follows from

```
r_s = 2GM/c^2
```

That means the black sphere on screen is the size the mass says it should be,
and the interesting radii land where they should. The photon sphere is at
1.5 r_s and the innermost stable circular orbit is at 3 r_s.

## The matter

Pressing F spawns a thin sheet of particles offset to one side of the hole and
moving past it. The one sided offset makes it so every particle then carries
angular momentum in the same direction, so a disc forms. Spawning them
symmetrically gives counter rotating streams and the result collapses into a
cone instead.

Each particle follows a timelike geodesic, integrated with RK4 in a compute
shader. The acceleration is Newtonian gravity plus the relativistic correction:

```
a = -(GM/r^2 + 3 M h^2 / r^4) * r_hat
```

The second term is the one that makes Mercury's orbit precess. It is also why
there is an innermost stable orbit at all, which Newtonian gravity does not
have. Particles that fall inside the horizon are deleted, as are ones that
leave the grid.

Gravity alone cannot make a disc, because energy is conserved along a geodesic so anything arriving from outside either falls in or leaves again. Real
accretion discs exist because gas loses energy so inside a radius there is a
damping term on the radial velocity, which circularises the orbits, and a much
weaker drag on the tangential velocity, which slowly drains them inward. The
disc forms, tightens, crosses the ISCO and plunges over about ninety seconds.

## The background

The stars and the shadow are raymarched. Every pixel integrates a null geodesic
backwards from the camera with RK4 and adaptive step size, using

```
a = -3 M h^2 / r^5 * r_vec
```

which is the same law with the Newtonian term removed, because for light the
entire deflection comes from the relativistic piece. Each ray either crosses
the horizon and comes back black, or escapes and samples a procedural starfield
in whatever direction it ended up pointing.

## The grid

The flat grid under the hole is not deformed. It is a perfectly ordinary square
grid and it stays that way. What moves is where each of its vertices appears,
using the thin lens equation

```
theta = (beta + sqrt(beta^2 + 4 theta_E^2)) / 2
```

solved per vertex in the vertex shader. Pressing space sets the mass to zero and
the grid snaps back to a plain square grid, which is a quick way to confirm the
distortion really is coming from lensing.

That equation is a quadratic and it has a second root, which is the image on the
other side of the hole. The particles are drawn twice, once for each root, which
is where the arc under the shadow comes from.

## Rendering

The scene renders into a floating point target, gets a bright pass and a
separable blur for bloom, then an ACES tonemap down to the screen. This is not
decoration. Particle colour comes from relativistic beaming and gravitational
redshift:

```
doppler   = 1 / (gamma * (1 - beta_los))
redshift  = sqrt(1 - r_s/r)
brightness = (doppler * redshift)^4
```

The fourth power means the side of the disc rotating toward the camera is many
times brighter than the receding side. In an eight bit target both sides clip to
white and the whole effect disappears, so the float target is used to makes it
visible at all.

Particles blend additively and are drawn as one pixel points, so density reads
as brightness.

## What it gets wrong

The hole does not spin. Everything is Schwarzschild, so there is no frame dragging and no ergosphere. The illusion is created only because the matter was thrown
in with angular momentum.

The hole also does not grow or gain angular momentum from what it swallows. Its
mass is a constant.

There are two different lensing models running at once. The background is
properly raymarched, while the particles and the grid use the weak field thin
lens. They disagree near the hole, and the raymarched shadow does not occlude
the particles because making it do so caused the lensed arcs to flicker.

The drag terms that make the disc form are a stand in for viscosity, not a
simulation of it. Particles never interact with each other.

---
