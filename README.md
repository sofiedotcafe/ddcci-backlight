<!-- markdownlint-disable MD033 MD013 -->

# ddcci-backlight

<a href="https://builtwithnix.org"><img src="https://builtwithnix.org/badge.svg" alt="Built with Nix" height="20"/></a>
<a href="https://rust-lang.org/"><img src="https://img.shields.io/badge/-Rust-%23c9d1d9?logo=rust&logoColor=black" alt="OpenTofu" height="20"/></a>
<a href="https://github.com/RedHatPride/open-source-transition-resources"><img src="https://pride-badges.pony.workers.dev/static/v1?&stripeWidth=6&labelColor=%23c9d1d9&stripeColors=5BCEFA,F5A9B8,FFFFFF,F5A9B8,5BCEFA" alt="Pride Badge" height="20"/></a>

A Rust Linux kernel module that exposes monitor brightness control through **DDC/CI**.  
It provides a kernel‑space implementation of the DDC/CI protocol, allowing user‑space tools to adjust external monitor brightness using a stable, low‑latency interface.

It lets your desktop environment control your monitor’s built‑in brightness setting, the same one you normally adjust through the monitor’s on‑screen menu, and exposes it as a standard brightness slider in GNOME, KDE, etc.

![Showcase](./assets/showcase.png)

---

## Features

> [!WARNING]
> This software is provided **without any warranty** under GPL‑2.0.  
> It performs direct DDC/CI communication with monitor firmware, and improper VCP handling, hardware non‑compliance, or unexpected I²C/DDC behavior **may result in permanent device damage**.  
> Use with caution and at your own risk.

- Kernel‑space DDC/CI communication  
- Rust implementation (no C glue)  
- Safe abstractions over I²C/DDC messaging  
- Exposes a simple sysfs interface for brightness control  
- Nix‑based reproducible build system  
- Out‑of‑tree module compatible with modern Linux kernels

---

## Requirements

- Linux kernel **with Rust support enabled**  
- Kernel headers matching your running kernel  
- Nix (optional, but recommended for building)  
- A monitor that supports DDC/CI brightness control

---

## Building (Nix)

To build the kernel module using Nix:

```bash
nix build .#ddci-backlight
```

The resulting module will be located at:

```bash
result/lib/modules/<kernel-version>/extra/ddcci_backlight.ko
```

---

## Loading the Module

### Use it in NixOS

You can consume this module from any external flake by adding it as an input:
nix

```nix
{
  inputs.ddci-backlight.url = "github:sofiedotcafe/ddci-backlight";
}
```

```nix
{
  boot = {
    kernelPackages = pkgs.linuxPackages_latest;
    extraModulePackages = [
      (pkgs.ddci-backlight.override {
        kernel = config.boot.kernelPackages;
      })
    ];

    kernelModules = [ "ddcci_backlight" ];
  };
}
```

---

## Usage

Once loaded, the module exposes a sysfs interface:

```bash
/sys/class/backlight/ddcci-backlight/
```

Common operations:

```bash
# Read brightness
cat /sys/class/backlight/ddcci-backlight/brightness

# Set brightness
echo 50 | sudo tee /sys/class/backlight/ddcci-backlight/brightness
```

---

## Development

For development, build the project using Nix

```bash
nix build .#default
```

---

## Code Of Conduct

This project follows the [Lix Code of Conduct/Community Standards](https://lix.systems).

## License

This repository is distributed under the GNU General Public License v2 (GPLv2).

GPL-2.0-only © 2026
