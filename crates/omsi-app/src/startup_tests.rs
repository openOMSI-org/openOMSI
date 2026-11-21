use super::*;

mod own_key_tests {
    /// OMSI's own file as it comes has no key of the player's own.
    #[test]
    fn the_stock_file_has_none_of_the_players() {
        let root = std::path::Path::new("../../../OMSI 2 Original");
        if !root.join("Inputs/keyboard.cfg").exists() {
            return;
        }
        let stock = omsi_cfg::original_keyboard_cfg(root);
        assert!(super::own_bindings_in(&stock, 0).is_empty());
        assert!(super::own_bindings_in(&stock, omsi_content::input::KEY_SHIFT).is_empty());
    }
}

mod window_tests {
    use super::software_adapter;

    /// The software renderers come after every graphics chip (#770).
    #[test]
    fn software_renderers_are_told_from_chips() {
        use wgpu::DeviceType::*;
        assert!(software_adapter("Microsoft Basic Render Driver", Cpu));
        assert!(software_adapter("Microsoft Basic Render Driver", Other));
        assert!(software_adapter("llvmpipe (LLVM 15.0.7, 256 bits)", Cpu));
        assert!(!software_adapter(
            "Intel(R) HD Graphics 2500",
            IntegratedGpu
        ));
        assert!(!software_adapter(
            "NVIDIA GeForce RTX 3050 Laptop GPU",
            DiscreteGpu
        ));
    }

    #[test]
    fn auto_opens_vulkan_only_on_a_discrete_amd_radeon() {
        use wgpu::DeviceType::{DiscreteGpu, IntegratedGpu};
        let amd = (DiscreteGpu, 0x1002, 0x744c);
        let apu = (IntegratedGpu, 0x1002, 0x164e);
        let nvidia = (DiscreteGpu, 0x10de, 0x2684);
        let pick = |wanted, mark: Option<&str>, a: &[_]| super::vulkan_adapter(wanted, mark, a);
        assert_eq!(pick("auto", None, &[apu, amd]), Some((0x1002, 0x744c)));
        assert_eq!(pick("", None, &[amd]), Some((0x1002, 0x744c)));
        assert_eq!(pick("auto", None, &[apu]), None);
        assert_eq!(pick("auto", None, &[apu, nvidia]), None);
        assert_eq!(pick("auto", None, &[nvidia, amd]), None);
        assert_eq!(pick("vulkan", None, &[amd]), None);
        assert_eq!(pick("dx12", None, &[amd]), None);
        let mark = super::vulkan_lost_mark(0x1002, 0x744c);
        assert_eq!(pick("auto", Some(&mark), &[amd]), None);
        assert_eq!(
            pick("auto", Some("0.0.1|1002-744c"), &[amd]),
            Some((0x1002, 0x744c))
        );
        assert_eq!(
            pick(
                "auto",
                Some(&super::vulkan_lost_mark(0x1002, 0x73bf)),
                &[amd]
            ),
            Some((0x1002, 0x744c))
        );
    }

    #[test]
    fn the_other_vulkan_adapters_are_tried_once_directx_12_could_not_draw() {
        use super::{GraphicsApi, vulkan_passed_over, vulkan_pin_after};
        use wgpu::Backend::{Dx12, Vulkan};
        let (radeon, apu) = ((0x1002, 0x744c), (0x1002, 0x164e));
        let [vulkan, dx12, gl] = [
            wgpu::Backends::VULKAN,
            wgpu::Backends::DX12,
            wgpu::Backends::GL,
        ]
        .map(|b| Some(GraphicsApi::Native(b)));
        let skipped =
            |pin, backend, (vendor, device)| vulkan_passed_over(pin, backend, vendor, device);
        let pin = vulkan_pin_after(Some(radeon), [None, vulkan].into_iter());
        assert_eq!(pin, Some(radeon));
        assert!(!skipped(pin, Vulkan, radeon));
        assert!(skipped(pin, Vulkan, apu));
        assert!(!skipped(pin, Dx12, apu));
        let pin = vulkan_pin_after(pin, [None, vulkan, dx12].into_iter());
        assert_eq!(pin, None);
        assert!(!skipped(pin, Vulkan, apu));
        assert_eq!(
            vulkan_pin_after(None, [None, vulkan, dx12, gl].into_iter()),
            None
        );
        assert!(!skipped(None, Vulkan, apu));
    }

    #[test]
    fn the_window_fits_a_small_screen_and_sits_in_its_middle() {
        // 1920 x 1200 at 125 %: 1600 x 900 points would be 2000 px wide
        let ((w, h), (x, y)) = super::fit_rect((1600.0, 900.0), (1920.0, 1200.0), 1.25);
        assert!(
            w * 1.25 <= 1920.0 * 0.9 + 1.0 && h * 1.25 <= 1200.0 * 0.85 + 1.0,
            "{w} x {h}"
        );
        assert!(((w / h) - 16.0 / 9.0).abs() < 0.01);
        assert!((x as f64 - (1920.0 - w * 1.25) / 2.0).abs() <= 1.0);
        assert!(y > 0);
        // a big screen keeps the size asked for
        let ((w, h), _) = super::fit_rect((1600.0, 900.0), (3840.0, 2160.0), 1.5);
        assert_eq!((w, h), (1600.0, 900.0));
    }
}

#[cfg(windows)]
mod cpu_time_tests {
    /// OMSI_PROFILE's CPU time is read on Windows, which has no `ps` (whose whole seconds
    /// elsewhere would not show 200 ms).
    #[test]
    fn the_process_cpu_time_is_read_and_grows() {
        let before = super::process_cpu_seconds().expect("CPU time");
        let t = std::time::Instant::now();
        let mut x = 0u64;
        while t.elapsed().as_millis() < 200 {
            x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
        }
        let after = super::process_cpu_seconds().expect("CPU time");
        assert!(after > before, "{before} -> {after}");
    }
}
