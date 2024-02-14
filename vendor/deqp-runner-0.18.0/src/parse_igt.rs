// Copyright (c) 2022 Collabora
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice (including the next
// paragraph) shall be included in all copies or substantial portions of the
// Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
// THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

use anyhow::{Context, Result};
use log::*;
use std::path::{Path, PathBuf};

use crate::parse::{ParserState, ResultParser};
use crate::{read_lines, BinaryTest, TestCase};

pub struct IgtResultParser {
    test_name: String,
}

impl IgtResultParser {
    pub fn new(name: &str) -> IgtResultParser {
        IgtResultParser {
            test_name: igt_sanitize_test_name(name),
        }
    }
}

impl ResultParser for IgtResultParser {
    fn initialize(&mut self) -> Option<ParserState> {
        Some(ParserState::BeginTest(self.test_name.to_owned()))
    }

    fn parse_line(&mut self, _: &str) -> Result<Option<ParserState>> {
        // IGT status is defined by the exit code, see handle_exit_status()
        Ok(None)
    }
}

/* Replace ',' to avoid conflicts in the csv file */
fn igt_sanitize_test_name(test: &str) -> String {
    test.replace(',', "-")
}

pub fn read_testlist_file(igt_test_folder: &std::path::Path) -> Result<String> {
    let path = igt_test_folder.join(Path::new("test-list.txt"));
    info!("... using {:?}", &path);
    std::fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))
}

pub fn igt_parse_testlist_file(file_content: &str) -> Result<Vec<&str>> {
    //let tests: Vec<&str> = file_content.split(" ").filter(|word| *word != "TESTLIST" && *word != "END").collect()?;
    let tests: Vec<&str> = file_content
        .split_whitespace()
        .filter(|word| !word.contains("TESTLIST") && !word.contains("END"))
        .collect();
    Ok(tests)
}

pub fn igt_parse_testcases_from_subtests(
    test: &str,
    subtests: Vec<String>,
) -> Result<Vec<crate::TestCase>> {
    if subtests.is_empty() {
        return Ok(vec![BinaryTest {
            name: test.to_string(),
            binary: test.to_string(),
            args: vec![],
        }
        .into()]);
    }

    let mut tests: Vec<TestCase> = Vec::new();
    for subtest in subtests {
        tests.push(
            BinaryTest {
                name: format!("{}@{}", test, subtest),
                binary: test.to_string(),
                args: vec!["--run-subtest".to_string(), subtest],
            }
            .into(),
        );
    }
    Ok(tests)
}

pub fn igt_parse_testcases_from_caselist(caselists: &[PathBuf]) -> Result<Vec<crate::TestCase>> {
    let test_names = read_lines(caselists)?;
    let mut tests: Vec<TestCase> = Vec::new();
    for test in test_names {
        if !test.contains('@') {
            tests.push(
                BinaryTest {
                    name: test.to_string(),
                    binary: test.to_string(),
                    args: vec![],
                }
                .into(),
            );
            continue;
        }
        let splited: Vec<&str> = test.split('@').collect();
        let binary = splited[0];
        let subtest = splited[1];

        tests.push(
            BinaryTest {
                name: test.to_string(),
                binary: binary.to_string(),
                args: vec!["--run-subtest".to_string(), subtest.to_string()],
            }
            .into(),
        );
    }
    Ok(tests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_igt_testlist() {
        let file_content = "
TESTLIST
core_auth core_getclient core_getstats core_getversion core_setmaster core_setmaster_vs_auth debugfs_test dmabuf device_reset drm_buddy drm_import_export drm_mm drm_read fbdev feature_discovery kms_3d kms_addfb_basic kms_async_flips kms_atomic kms_atomic_interruptible kms_atomic_transition kms_bw kms_concurrent kms_content_protection kms_cursor_crc kms_cursor_edge_walk kms_cursor_legacy kms_dither kms_display_modes kms_dp_aux_dev kms_dp_tiled_display kms_flip kms_flip_event_leak kms_force_connector_basic kms_getfb kms_hdmi_inject kms_hdr kms_invalid_mode kms_lease kms_multipipe_modeset kms_panel_fitting kms_pipe_crc_basic kms_plane kms_plane_alpha_blend kms_plane_cursor kms_plane_lowres kms_plane_multiple kms_plane_scaling kms_prime kms_prop_blob kms_properties kms_rmfb kms_rotation_crc kms_scaling_modes kms_selftest kms_sequence kms_setmode kms_sysfs_edid_timing kms_tv_load_detect kms_universal_plane kms_vblank kms_vrr kms_writeback meta_test panfrost_get_param panfrost_gem_new panfrost_prime panfrost_submit prime_busy prime_mmap prime_mmap_coherency prime_mmap_kms prime_self_import prime_udl prime_vgem syncobj_basic syncobj_wait syncobj_timeline template tools_test v3d_get_bo_offset v3d_get_param v3d_mmap vc4_create_bo vc4_dmabuf_poll vc4_label_bo vc4_lookup_fail vc4_purgeable_bo vc4_tiling vc4_wait_bo vc4_wait_seqno vgem_basic vgem_slow prime_nv_api prime_nv_pcopy prime_nv_test nouveau_crc api_intel_allocator api_intel_bb gen3_mixed_blits gen3_render_linear_blits gen3_render_mixed_blits gen3_render_tiledx_blits gen3_render_tiledy_blits gem_bad_reloc gem_basic gem_blits gem_busy gem_caching gem_ccs gem_close gem_close_race gem_concurrent_blit gem_cs_tlb gem_ctx_bad_destroy gem_ctx_create gem_ctx_engines gem_ctx_exec gem_ctx_isolation gem_ctx_param gem_ctx_persistence gem_ctx_shared gem_ctx_switch gem_evict_alignment gem_evict_everything gem_exec_alignment gem_exec_async gem_exec_await gem_exec_basic gem_exec_big gem_exec_capture gem_exec_create gem_exec_endless gem_exec_fair gem_exec_fence gem_exec_flush gem_exec_gttfill gem_exec_latency gem_exec_lut_handle gem_exec_nop gem_exec_parallel gem_exec_params gen7_exec_parse gen9_exec_parse gem_exec_reloc gem_exec_schedule gem_exec_store gem_exec_suspend gem_exec_whisper gem_fd_exhaustion gem_fence_thrash gem_fence_upload gem_fenced_exec_thrash gem_flink_basic gem_flink_race gem_gpgpu_fill gem_gtt_cpu_tlb gem_gtt_hog gem_gtt_speed gem_huc_copy gem_linear_blits gem_lmem_swapping gem_lut_handle gem_madvise gem_media_fill gem_media_vme gem_mmap gem_mmap_gtt gem_mmap_wc gem_partial_pwrite_pread gem_pipe_control_store_loop gem_ppgtt gem_pread gem_pread_after_blit gem_pwrite gem_pwrite_snooped gem_pxp gem_read_read_speed gem_readwrite gem_reg_read gem_render_copy gem_render_copy_redux gem_render_linear_blits gem_render_tiled_blits gem_request_retire gem_reset_stats gem_ringfill gem_set_tiling_vs_blt gem_set_tiling_vs_gtt gem_set_tiling_vs_pwrite gem_shrink gem_softpin gem_spin_batch gem_streaming_writes gem_sync gem_tiled_blits gem_tiled_fence_blits gem_tiled_partial_pwrite_pread gem_tiled_pread_basic gem_tiled_pread_pwrite gem_tiled_swapping gem_tiled_wb gem_tiled_wc gem_tiling_max_stride gem_unfence_active_buffers gem_unref_active_buffers gem_userptr_blits gem_vm_create gem_wait gem_watchdog gem_workarounds i915_fb_tiling i915_getparams_basic i915_hangman i915_module_load i915_pciid i915_pm_backlight i915_pm_lpsp i915_pm_rpm i915_pm_dc i915_pm_rps i915_pm_sseu i915_query i915_selftest i915_suspend kms_big_fb kms_big_joiner kms_busy kms_ccs kms_cdclk kms_draw_crc kms_dsc kms_fbcon_fbt kms_fence_pin_leak kms_flip_scaled_crc kms_flip_tiling kms_frontbuffer_tracking kms_legacy_colorkey kms_mmap_write_crc kms_pipe_b_c_ivb kms_psr kms_psr2_su kms_psr2_sf kms_psr_stress_test kms_pwrite_crc sysfs_clients sysfs_defaults sysfs_heartbeat_interval sysfs_preempt_timeout sysfs_timeslice_duration msm_mapping msm_recovery msm_submit drm_fdinfo dumb_buffer gem_create gem_ctx_freq gem_ctx_sseu gem_eio gem_exec_balancer gem_mmap_offset i915_pm_rc6_residency perf_pmu perf core_hotunplug testdisplay kms_color sw_sync amdgpu/amd_abm amdgpu/amd_assr amdgpu/amd_basic amdgpu/amd_bypass amdgpu/amd_color amdgpu/amd_cs_nop amdgpu/amd_hotplug amdgpu/amd_info amdgpu/amd_prime amdgpu/amd_max_bpc amdgpu/amd_module_load amdgpu/amd_mem_leak amdgpu/amd_link_settings amdgpu/amd_vrr_range amdgpu/amd_mode_switch amdgpu/amd_dp_dsc amdgpu/amd_psr amdgpu/amd_plane amdgpu/amd_ilr gem_concurrent_all
END TESTLIST";
        let results = igt_parse_testlist_file(file_content).unwrap();
        assert_eq!(
            results,
            vec![
                "core_auth",
                "core_getclient",
                "core_getstats",
                "core_getversion",
                "core_setmaster",
                "core_setmaster_vs_auth",
                "debugfs_test",
                "dmabuf",
                "device_reset",
                "drm_buddy",
                "drm_import_export",
                "drm_mm",
                "drm_read",
                "fbdev",
                "feature_discovery",
                "kms_3d",
                "kms_addfb_basic",
                "kms_async_flips",
                "kms_atomic",
                "kms_atomic_interruptible",
                "kms_atomic_transition",
                "kms_bw",
                "kms_concurrent",
                "kms_content_protection",
                "kms_cursor_crc",
                "kms_cursor_edge_walk",
                "kms_cursor_legacy",
                "kms_dither",
                "kms_display_modes",
                "kms_dp_aux_dev",
                "kms_dp_tiled_display",
                "kms_flip",
                "kms_flip_event_leak",
                "kms_force_connector_basic",
                "kms_getfb",
                "kms_hdmi_inject",
                "kms_hdr",
                "kms_invalid_mode",
                "kms_lease",
                "kms_multipipe_modeset",
                "kms_panel_fitting",
                "kms_pipe_crc_basic",
                "kms_plane",
                "kms_plane_alpha_blend",
                "kms_plane_cursor",
                "kms_plane_lowres",
                "kms_plane_multiple",
                "kms_plane_scaling",
                "kms_prime",
                "kms_prop_blob",
                "kms_properties",
                "kms_rmfb",
                "kms_rotation_crc",
                "kms_scaling_modes",
                "kms_selftest",
                "kms_sequence",
                "kms_setmode",
                "kms_sysfs_edid_timing",
                "kms_tv_load_detect",
                "kms_universal_plane",
                "kms_vblank",
                "kms_vrr",
                "kms_writeback",
                "meta_test",
                "panfrost_get_param",
                "panfrost_gem_new",
                "panfrost_prime",
                "panfrost_submit",
                "prime_busy",
                "prime_mmap",
                "prime_mmap_coherency",
                "prime_mmap_kms",
                "prime_self_import",
                "prime_udl",
                "prime_vgem",
                "syncobj_basic",
                "syncobj_wait",
                "syncobj_timeline",
                "template",
                "tools_test",
                "v3d_get_bo_offset",
                "v3d_get_param",
                "v3d_mmap",
                "vc4_create_bo",
                "vc4_dmabuf_poll",
                "vc4_label_bo",
                "vc4_lookup_fail",
                "vc4_purgeable_bo",
                "vc4_tiling",
                "vc4_wait_bo",
                "vc4_wait_seqno",
                "vgem_basic",
                "vgem_slow",
                "prime_nv_api",
                "prime_nv_pcopy",
                "prime_nv_test",
                "nouveau_crc",
                "api_intel_allocator",
                "api_intel_bb",
                "gen3_mixed_blits",
                "gen3_render_linear_blits",
                "gen3_render_mixed_blits",
                "gen3_render_tiledx_blits",
                "gen3_render_tiledy_blits",
                "gem_bad_reloc",
                "gem_basic",
                "gem_blits",
                "gem_busy",
                "gem_caching",
                "gem_ccs",
                "gem_close",
                "gem_close_race",
                "gem_concurrent_blit",
                "gem_cs_tlb",
                "gem_ctx_bad_destroy",
                "gem_ctx_create",
                "gem_ctx_engines",
                "gem_ctx_exec",
                "gem_ctx_isolation",
                "gem_ctx_param",
                "gem_ctx_persistence",
                "gem_ctx_shared",
                "gem_ctx_switch",
                "gem_evict_alignment",
                "gem_evict_everything",
                "gem_exec_alignment",
                "gem_exec_async",
                "gem_exec_await",
                "gem_exec_basic",
                "gem_exec_big",
                "gem_exec_capture",
                "gem_exec_create",
                "gem_exec_endless",
                "gem_exec_fair",
                "gem_exec_fence",
                "gem_exec_flush",
                "gem_exec_gttfill",
                "gem_exec_latency",
                "gem_exec_lut_handle",
                "gem_exec_nop",
                "gem_exec_parallel",
                "gem_exec_params",
                "gen7_exec_parse",
                "gen9_exec_parse",
                "gem_exec_reloc",
                "gem_exec_schedule",
                "gem_exec_store",
                "gem_exec_suspend",
                "gem_exec_whisper",
                "gem_fd_exhaustion",
                "gem_fence_thrash",
                "gem_fence_upload",
                "gem_fenced_exec_thrash",
                "gem_flink_basic",
                "gem_flink_race",
                "gem_gpgpu_fill",
                "gem_gtt_cpu_tlb",
                "gem_gtt_hog",
                "gem_gtt_speed",
                "gem_huc_copy",
                "gem_linear_blits",
                "gem_lmem_swapping",
                "gem_lut_handle",
                "gem_madvise",
                "gem_media_fill",
                "gem_media_vme",
                "gem_mmap",
                "gem_mmap_gtt",
                "gem_mmap_wc",
                "gem_partial_pwrite_pread",
                "gem_pipe_control_store_loop",
                "gem_ppgtt",
                "gem_pread",
                "gem_pread_after_blit",
                "gem_pwrite",
                "gem_pwrite_snooped",
                "gem_pxp",
                "gem_read_read_speed",
                "gem_readwrite",
                "gem_reg_read",
                "gem_render_copy",
                "gem_render_copy_redux",
                "gem_render_linear_blits",
                "gem_render_tiled_blits",
                "gem_request_retire",
                "gem_reset_stats",
                "gem_ringfill",
                "gem_set_tiling_vs_blt",
                "gem_set_tiling_vs_gtt",
                "gem_set_tiling_vs_pwrite",
                "gem_shrink",
                "gem_softpin",
                "gem_spin_batch",
                "gem_streaming_writes",
                "gem_sync",
                "gem_tiled_blits",
                "gem_tiled_fence_blits",
                "gem_tiled_partial_pwrite_pread",
                "gem_tiled_pread_basic",
                "gem_tiled_pread_pwrite",
                "gem_tiled_swapping",
                "gem_tiled_wb",
                "gem_tiled_wc",
                "gem_tiling_max_stride",
                "gem_unfence_active_buffers",
                "gem_unref_active_buffers",
                "gem_userptr_blits",
                "gem_vm_create",
                "gem_wait",
                "gem_watchdog",
                "gem_workarounds",
                "i915_fb_tiling",
                "i915_getparams_basic",
                "i915_hangman",
                "i915_module_load",
                "i915_pciid",
                "i915_pm_backlight",
                "i915_pm_lpsp",
                "i915_pm_rpm",
                "i915_pm_dc",
                "i915_pm_rps",
                "i915_pm_sseu",
                "i915_query",
                "i915_selftest",
                "i915_suspend",
                "kms_big_fb",
                "kms_big_joiner",
                "kms_busy",
                "kms_ccs",
                "kms_cdclk",
                "kms_draw_crc",
                "kms_dsc",
                "kms_fbcon_fbt",
                "kms_fence_pin_leak",
                "kms_flip_scaled_crc",
                "kms_flip_tiling",
                "kms_frontbuffer_tracking",
                "kms_legacy_colorkey",
                "kms_mmap_write_crc",
                "kms_pipe_b_c_ivb",
                "kms_psr",
                "kms_psr2_su",
                "kms_psr2_sf",
                "kms_psr_stress_test",
                "kms_pwrite_crc",
                "sysfs_clients",
                "sysfs_defaults",
                "sysfs_heartbeat_interval",
                "sysfs_preempt_timeout",
                "sysfs_timeslice_duration",
                "msm_mapping",
                "msm_recovery",
                "msm_submit",
                "drm_fdinfo",
                "dumb_buffer",
                "gem_create",
                "gem_ctx_freq",
                "gem_ctx_sseu",
                "gem_eio",
                "gem_exec_balancer",
                "gem_mmap_offset",
                "i915_pm_rc6_residency",
                "perf_pmu",
                "perf",
                "core_hotunplug",
                "testdisplay",
                "kms_color",
                "sw_sync",
                "amdgpu/amd_abm",
                "amdgpu/amd_assr",
                "amdgpu/amd_basic",
                "amdgpu/amd_bypass",
                "amdgpu/amd_color",
                "amdgpu/amd_cs_nop",
                "amdgpu/amd_hotplug",
                "amdgpu/amd_info",
                "amdgpu/amd_prime",
                "amdgpu/amd_max_bpc",
                "amdgpu/amd_module_load",
                "amdgpu/amd_mem_leak",
                "amdgpu/amd_link_settings",
                "amdgpu/amd_vrr_range",
                "amdgpu/amd_mode_switch",
                "amdgpu/amd_dp_dsc",
                "amdgpu/amd_psr",
                "amdgpu/amd_plane",
                "amdgpu/amd_ilr",
                "gem_concurrent_all"
            ],
        );
    }

    #[test]
    fn testcases_from_subtests_empty() {
        let test = "code_getclient";
        let results = igt_parse_testcases_from_subtests(test, vec![]).unwrap();
        assert_eq!(
            results,
            vec![BinaryTest {
                name: "code_getclient".to_string(),
                binary: "code_getclient".to_string(),
                args: vec![],
            }
            .into()]
        );
    }

    #[test]
    fn testcases_from_subtests() {
        let test = "code_auth";
        let subtests = vec![
            "getclient-simple".to_string(),
            "getclient-master-drop".to_string(),
            "basic-auth".to_string(),
            "many-magics".to_string(),
        ];
        let results = igt_parse_testcases_from_subtests(test, subtests).unwrap();
        assert_eq!(
            results,
            vec![
                BinaryTest {
                    name: "code_auth@getclient-simple".to_string(),
                    binary: "code_auth".to_string(),
                    args: vec!["--run-subtest".to_string(), "getclient-simple".to_string()],
                }
                .into(),
                BinaryTest {
                    name: "code_auth@getclient-master-drop".to_string(),
                    binary: "code_auth".to_string(),
                    args: vec![
                        "--run-subtest".to_string(),
                        "getclient-master-drop".to_string()
                    ],
                }
                .into(),
                BinaryTest {
                    name: "code_auth@basic-auth".to_string(),
                    binary: "code_auth".to_string(),
                    args: vec!["--run-subtest".to_string(), "basic-auth".to_string()],
                }
                .into(),
                BinaryTest {
                    name: "code_auth@many-magics".to_string(),
                    binary: "code_auth".to_string(),
                    args: vec!["--run-subtest".to_string(), "many-magics".to_string()],
                }
                .into(),
            ]
        );
    }
}
