# Smarticles (fork)

A Rust port of [Brainxyz's Artificial Life](https://www.youtube.com/watch?v=0Kx4Y9TVMGg)
simulator with some fun features.

> _A simple program to simulate primitive Artificial Life using simple rules of
> attraction or repulsion among atom-like particles, producing complex
> self-organzing life-like patterns._
>
> – from the [original repository](https://github.com/hunar4321/life_code)

![animation of the app simulating particles](./img/app_anim.gif)

## What has changed/will change compared to the original in this fork

- [x] add more particle types
- [x] make it possible to move around and zoom

## Running the App

To run this, you will need Rust installed, which you can do by following the
installation instructions on the [Rust website](https://www.rust-lang.org/).
You should then have `cargo` installed, which is the command line program
for managing and running Rust projects.

You can check your version of `cargo` in the command line:

```commandline
cargo --version
cargo 1.75.0 (1d8b05cdd 2023-11-20)
```

Once done, download or clone this repository to your preferred location and
run the program using `cargo` like so:

```commandline
cd ~/path/to/smarticles-fork
cargo run
```

## How to Use It

First, watch it in action. Press the `Randomize` button, which will spawn a
bunch of particles with randomized settings. Then, press `Play` to run the
simulation.

![screenshot of the app's basic controls](./img/random_play.png)

Try randomizing it a few times and seeing what kind of results you get.

![animation of the app simulating particles](./img/app_anim2.gif)

There are 8 particle types. You can change the behavior of each with respect to any other with the sliders:

![screenshot of particle's parameters](./img/params.png)

`Power` is the particle's attraction to particles of the other type. A positive
number means it is attracted to them, and negative means it is repulsed away.
`Radius` is how far away the particle can sense particles of that type.

You can adjust these parameters while the simulation is running if you want to
see the effect they have:

## Sharing Simulations

The `Seed` field is the _D.N.A_ of your particle system. It contains all the
information needed to replicate the current simulation. Pressing `Randomize`
will give you random seeds, but you can also enter a custom one.

What does _your_ name look like?

![simulation using "chevy" as the seed](./img/custom_seed.gif)

> ☝️ literally the inside of my brain ☝️

If you start adjusting parameters, you'll notice the seed changes to a code
that begins with the `@` symbol. These are custom-encoded simulations, which
you can share by copying the entire code.

The code will be partially cut-off by the textbox, so make sure you select it all
before copying.

![screenshot of particle's parameters](./img/custom_code.png)
