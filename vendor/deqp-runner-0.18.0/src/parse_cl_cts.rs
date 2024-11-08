//! Parses the output of OpenCL CTS test binaries into a deqp-runner status

use crate::{
    parse::{ParserState, ResultParser},
    TestStatus,
};
use anyhow::Result;
use log::*;
use regex::Regex;

impl TestStatus {
    // Parses the status name from a CL CTS binary's output.
    fn from_cl_cts_str(input: &str) -> Result<TestStatus, anyhow::Error> {
        match input {
            "PASSED" => Ok(TestStatus::Pass),
            "passed" => Ok(TestStatus::Pass),
            "FAILED" => Ok(TestStatus::Fail),
            "SKIPPED" => Ok(TestStatus::Skip),
            _ => anyhow::bail!("unknown CL CTS status '{}'", input),
        }
    }
}

pub struct ClCtsResultParser {
    test_name: String,
}

impl ClCtsResultParser {
    pub fn new(name: &str) -> ClCtsResultParser {
        ClCtsResultParser {
            test_name: name.to_string(),
        }
    }
}

impl ResultParser for ClCtsResultParser {
    fn initialize(&mut self) -> Option<ParserState> {
        Some(ParserState::BeginTest(self.test_name.to_owned()))
    }

    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>> {
        lazy_static! {
            static ref MULTI_STATUS_RE: Regex =
                Regex::new(r#"^([A-Z]*) ([0-9]*) of ([0-9]*) tests.$"#).unwrap();
            static ref STATUS_RE: Regex = Regex::new(r#"^([A-Z]*) test.$"#).unwrap();
            static ref SUB_STATUS_RE: Regex = Regex::new(r"^(\w+) (passed|FAILED)$").unwrap();
        }

        // Should be able to use alterntion ('|') to collapse these two
        // regexes, but I didn't get it to work.
        let all_status = MULTI_STATUS_RE
            .captures(line)
            .or_else(|| STATUS_RE.captures(line))
            .map(|cap| cap[1].to_owned());

        if let Some(all_status) = &all_status {
            let status = TestStatus::from_cl_cts_str(all_status).unwrap_or_else(|x| {
                error!("{}", x);
                TestStatus::Crash
            });
            return Ok(Some(ParserState::EndTest(status)));
        }

        if let Some(cap) = SUB_STATUS_RE.captures(line) {
            if let (Some(test), Some(status)) = (cap.get(1), cap.get(2)) {
                let status = TestStatus::from_cl_cts_str(status.as_str()).unwrap_or_else(|x| {
                    error!("{}", x);
                    TestStatus::Crash
                });
                // the buffer test sometimes prints "test passed" in between starting a test and ending it.
                if test.as_str() == "test" {
                    return Ok(None);
                }
                return Ok(Some(ParserState::SubTest(test.as_str().to_owned(), status)));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaselistResult, TestResult};
    use std::time::Duration;

    fn parse_cl_results(output: &mut &[u8]) -> CaselistResult {
        let parser = ClCtsResultParser::new("test");
        parser.parse(output).expect("Parser error")
    }

    fn result(status: TestStatus, subtests: Vec<TestResult>) -> Vec<TestResult> {
        vec![TestResult {
            name: "test".to_string(),
            status,
            duration: Duration::new(0, 0),
            subtests,
        }]
    }

    fn subtest(name: &str, status: TestStatus) -> TestResult {
        TestResult {
            name: name.to_string(),
            status,
            duration: Duration::new(0, 0),
            subtests: vec![],
        }
    }

    #[test]
    // Test a mix of subtest pass/fails, that result in an overall fail
    fn parse_contractions_fail() {
        let output = r#"
sizeof( void*) = 8
ARCH:	x86_64
system name = Linux
node name   = wildbow
release     = 5.19.0-2-amd64
version     = #1 SMP PREEMPT_DYNAMIC Debian 5.19.11-1 (2022-09-24)
machine     = x86_64
    Initializing random seed to 0.
Requesting Default device based on command line for platform index 0 and device index 0
Compute Device Name = llvmpipe (LLVM 14.0.6, 256 bits), Compute Device Vendor = FIXME, Compute Device Version = OpenCL 3.0 CLVK on Vulkan v1.3.230 driver 1, CL C Version = OpenCL C 1.2 CLVK on Vulkan v1.3.230 driver 1
Device latest conformance version passed: FIXME
Supports single precision denormals: NO
sizeof( void*) = 8  (host)
sizeof( void*) = 4  (device)

Compute Device info:
    Device Name: llvmpipe (LLVM 14.0.6, 256 bits)
    Vendor: FIXME
    Device Version: OpenCL 3.0 CLVK on Vulkan v1.3.230 driver 1
    CL C Version: OpenCL C 1.2 CLVK on Vulkan v1.3.230 driver 1
    Driver Version: 3.0 CLVK on Vulkan v1.3.230 driver 1
    Subnormal values supported? NO
    Testing with FTZ mode ON? YES
    Testing Doubles? NO
    Random Number seed: 0x1a06cc94


contractions_float_0...
4) Error for float kernel1: 0x1.0b98b4p-18 * 0x1.1020a2p-109 + -0x0p+0 =  *0x0p+0 vs. 0x1.1c745cp-127
contractions_float_0 FAILED
contractions_float_1...
4) Error for float kernel2: 0x1.0b98b4p-18 * 0x1.1020a2p-109 - 0x0p+0 =  *0x0p+0 vs. 0x1.1c745cp-127
contractions_float_1 FAILED
contractions_float_2...
4) Error for float kernel3: -0x0p+0 + 0x1.0b98b4p-18 * 0x1.1020a2p-109 =  *0x0p+0 vs. 0x1.1c745cp-127
contractions_float_2 FAILED
contractions_float_3...
4) Error for float kernel4: 0x0p+0 - 0x1.0b98b4p-18 * 0x1.1020a2p-109 =  *0x0p+0 vs. -0x1.1c745cp-127
contractions_float_3 FAILED
contractions_float_4...
4) Error for float kernel5: -(0x1.0b98b4p-18 * 0x1.1020a2p-109 + -0x0p+0) =  *-0x0p+0 vs. -0x1.1c745cp-127
contractions_float_4 FAILED
contractions_float_5...
4) Error for float kernel6: -(0x1.0b98b4p-18 * 0x1.1020a2p-109 - 0x0p+0) =  *-0x0p+0 vs. -0x1.1c745cp-127
contractions_float_5 FAILED
contractions_float_6...
4) Error for float kernel7: -(-0x0p+0 + 0x1.0b98b4p-18 * 0x1.1020a2p-109) =  *-0x0p+0 vs. -0x1.1c745cp-127
contractions_float_6 FAILED
contractions_float_7...
4) Error for float kernel8: -(0x0p+0 - 0x1.0b98b4p-18 * 0x1.1020a2p-109) =  *-0x0p+0 vs. 0x1.1c745cp-127
contractions_float_7 FAILED
contractions_double_0...
Double is not supported, test not run.
contractions_double_0 passed
contractions_double_1...
Double is not supported, test not run.
contractions_double_1 passed
contractions_double_2...
Double is not supported, test not run.
contractions_double_2 passed
contractions_double_3...
Double is not supported, test not run.
contractions_double_3 passed
contractions_double_4...
Double is not supported, test not run.
contractions_double_4 passed
contractions_double_5...
Double is not supported, test not run.
contractions_double_5 passed
contractions_double_6...
Double is not supported, test not run.
contractions_double_6 passed
contractions_double_7...
Double is not supported, test not run.
contractions_double_7 passed
PASSED sub-test.
FAILED 8 of 16 tests."#;

        let mut subtest_results = Vec::new();
        for i in 0..8 {
            subtest_results.push(subtest(
                &format!("contractions_float_{}", i),
                TestStatus::Fail,
            ));
        }
        for i in 0..8 {
            subtest_results.push(subtest(
                &format!("contractions_double_{}", i),
                TestStatus::Pass,
            ));
        }
        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(TestStatus::Fail, subtest_results),
        );
    }

    #[test]
    // Test a skipped test
    fn parse_workgroups_skip() {
        let output = r#"
Initializing random seed to 0.
Requesting Default device based on command line for platform index 0 and device index 0
Compute Device Name = AMD Radeon RX Vega (RADV VEGA10), Compute Device Vendor = FIXME, Compute Device Version = OpenCL 3.0 CLVK on Vulkan v1.3.230 driver 92282979, CL C Version = OpenCL C 1.2 CLVK on Vulkan v1.3.230 driver 92282979
Device latest conformance version passed: FIXME
Supports single precision denormals: NO
sizeof( void*) = 8  (host)
sizeof( void*) = 4  (device)
Test skipped while initialization
SKIPPED 17 of 17 tests.
"#;

        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(TestStatus::Skip, vec!()),
        );
    }

    #[test]
    // This one failed my first regex.
    fn parse_mipmaps_pass() {
        let output = r#"
Initializing random seed to 0.
Requesting Default device based on command line for platform index 0 and device index 0
Compute Device Name = AMD Radeon RX Vega (vega10, LLVM 14.0.6, DRM 3.48, 6.0.0-2-amd64), Compute Device Vendor = AMD, Compute Device Version = OpenCL 3.0, CL C Version = OpenCL C 1.2
Device latest conformance version passed: v0000-01-01-00
Supports single precision denormals: NO
sizeof( void*) = 8  (host)
sizeof( void*) = 8  (device)
1D...
-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

READ_WRITE_IMAGE tests skipped, not supported.
1D test not supported
2D...
-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

READ_WRITE_IMAGE tests skipped, not supported.
2D test not supported
3D...
-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

READ_WRITE_IMAGE tests skipped, not supported.
3D test not supported
1Darray...
-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

READ_WRITE_IMAGE tests skipped, not supported.
1Darray test not supported
2Darray...
-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

-----------------------------------------------------
This device does not support cl_khr_mipmap_image.
Skipping mipmapped image test.
-----------------------------------------------------

READ_WRITE_IMAGE tests skipped, not supported.
2Darray test not supported
cl_image_requirements_size_ext_negative...
Extension cl_ext_image_requirements_info not availablecl_image_requirements_size_ext_negative test not supported
cl_image_requirements_size_ext_consistency...
Extension cl_ext_image_requirements_info not availablecl_image_requirements_size_ext_consistency test not supported
clGetImageRequirementsInfoEXT_negative...
Extension cl_ext_image_requirements_info not availableclGetImageRequirementsInfoEXT_negative test not supported
cl_image_requirements_max_val_ext_negative...
Extension cl_ext_image_requirements_info not availablecl_image_requirements_max_val_ext_negative test not supported
cl_image_requirements_max_val_ext_positive...
Extension cl_ext_image_requirements_info not availablecl_image_requirements_max_val_ext_positive test not supported
image2d_from_buffer_positive...
Extension cl_khr_image2d_from_buffer not availableimage2d_from_buffer_positive test not supported
memInfo_image_from_buffer_positive...
Extension cl_ext_image_requirements_info not availablememInfo_image_from_buffer_positive test not supported
imageInfo_image_from_buffer_positive...
Extension cl_ext_image_requirements_info not availableimageInfo_image_from_buffer_positive test not supported
image_from_buffer_alignment_negative...
Extension cl_ext_image_requirements_info not availableimage_from_buffer_alignment_negative test not supported
image_from_small_buffer_negative...
Extension cl_ext_image_requirements_info not availableimage_from_small_buffer_negative test not supported
image_from_buffer_fill_positive...
Extension cl_ext_image_requirements_info not availableimage_from_buffer_fill_positive test not supported
image_from_buffer_read_positive...
Extension cl_ext_image_requirements_info not availableimage_from_buffer_read_positive test not supported
PASSED sub-test.
PASSED test.
"#;

        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(TestStatus::Pass, vec!()),
        );
    }

    #[test]
    // This one failed my first regex.
    fn parse_atomics_pass() {
        let output = r#"
Initializing random seed to 0.
Requesting Default device based on command line for platform index 0 and device index 0
Compute Device Name = AMD Radeon RX Vega (vega10, LLVM 14.0.6, DRM 3.48, 6.0.0-2-amd64), Compute Device Vendor = AMD, Compute Device Version = OpenCL 3.0, CL C Version = OpenCL C 1.2
Device latest conformance version passed: v0000-01-01-00
Supports single precision denormals: NO
sizeof( void*) = 8  (host)
sizeof( void*) = 8  (device)
atomic_add...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_add passed
atomic_sub...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_sub passed
atomic_xchg...
    Testing atom_ functions...
    global int32 ...	(thread count 64, group size 64)
    global uint32...	(thread count 64, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 64, group size 64)
    global uint32...	(thread count 64, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    global float...	(thread count 64, group size 64)
    local float ...	(thread count 64, group size 64)
atomic_xchg passed
atomic_min...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_min passed
atomic_max...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_max passed
atomic_inc...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_inc passed
atomic_dec...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_dec passed
atomic_cmpxchg...
    Testing atom_ functions...
    global int32 ...	(thread count 64, group size 64)
    global uint32...	(thread count 64, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 64, group size 64)
    global uint32...	(thread count 64, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_cmpxchg passed
atomic_and...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_and passed
atomic_or...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_or passed
atomic_xor...
    Testing atom_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
    Global int64 not supported
    Local int64 not supported
    Testing atomic_ functions...
    global int32 ...	(thread count 16384, group size 64)
    global uint32...	(thread count 16384, group size 64)
    local int32  ...	(thread count 64, group size 64)
    local uint32 ...	(thread count 64, group size 64)
atomic_xor passed
atomic_add_index...
Execute global_threads:2048 local_threads:64
add_index_test passed. Each thread used exactly one index.
atomic_add_index passed
atomic_add_index_bin...
add_index_bin_test with 2048 elements:
Execute global_threads:2048 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 4096 elements:
Execute global_threads:4096 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 8192 elements:
Execute global_threads:8192 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 16384 elements:
Execute global_threads:16384 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 32768 elements:
Execute global_threads:32768 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 65536 elements:
Execute global_threads:65536 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 131072 elements:
Execute global_threads:131072 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 262144 elements:
Execute global_threads:262144 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 524288 elements:
Execute global_threads:524288 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
add_index_bin_test with 1048576 elements:
Execute global_threads:1048576 local_threads:64
add_index_bin_test passed. Each item was put in the correct bin in parallel.
atomic_add_index_bin passed
PASSED sub-test.
PASSED 13 of 13 tests.
"#;

        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(
                TestStatus::Pass,
                vec!(
                    subtest("atomic_add", TestStatus::Pass),
                    subtest("atomic_sub", TestStatus::Pass),
                    subtest("atomic_xchg", TestStatus::Pass),
                    subtest("atomic_min", TestStatus::Pass),
                    subtest("atomic_max", TestStatus::Pass),
                    subtest("atomic_inc", TestStatus::Pass),
                    subtest("atomic_dec", TestStatus::Pass),
                    subtest("atomic_cmpxchg", TestStatus::Pass),
                    subtest("atomic_and", TestStatus::Pass),
                    subtest("atomic_or", TestStatus::Pass),
                    subtest("atomic_xor", TestStatus::Pass),
                    subtest("atomic_add_index", TestStatus::Pass),
                    subtest("atomic_add_index_bin", TestStatus::Pass)
                )
            ),
        );
    }

    #[test]
    fn parse_buffer() {
        // This is a subset of the test's output, where it has this "test passed" that we want to ignore (it's not a subtest result)
        let output = r#"
mem_read_write_flags...
test passed
mem_read_write_flags passed
mem_write_only_flags...
mem_write_only_flags passed
mem_read_only_flags...
test passed
mem_read_only_flags passed
mem_copy_host_flags...
test passed
mem_copy_host_flags passed
mem_alloc_ref_flags...
test passed
mem_alloc_ref_flags passed
array_info_size...
 CL_MEM_SIZE passed.
array_info_size passed
PASSED sub-test.
PASSED 91 of 91 tests.
"#;

        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(
                TestStatus::Pass,
                vec!(
                    subtest("mem_read_write_flags", TestStatus::Pass),
                    subtest("mem_write_only_flags", TestStatus::Pass),
                    subtest("mem_read_only_flags", TestStatus::Pass),
                    subtest("mem_copy_host_flags", TestStatus::Pass),
                    subtest("mem_alloc_ref_flags", TestStatus::Pass),
                    subtest("array_info_size", TestStatus::Pass),
                )
            ),
        );
    }

    #[test]
    fn parse_partial() {
        let output = "contractions_float_0...";

        assert_eq!(
            parse_cl_results(&mut output.as_bytes()).results,
            result(TestStatus::Crash, vec!()),
        );
    }
}
