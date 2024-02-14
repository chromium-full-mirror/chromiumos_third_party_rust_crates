use crate::parse::{ParserState, ResultParser};
use crate::TestStatus;
use anyhow::Result;
use regex::Regex;

impl TestStatus {
    // Parses the status name from dEQP's output.  See `s_qpTestResultMap` in
    // `qpTestLog.c`.
    fn from_deqp_str(input: &str) -> Result<TestStatus, anyhow::Error> {
        match input {
            "Pass" => Ok(TestStatus::Pass),
            "Fail" => Ok(TestStatus::Fail),
            "QualityWarning" => Ok(TestStatus::Warn),
            "CompatibilityWarning" => Ok(TestStatus::Warn),
            "Pending" => Ok(TestStatus::Fail),
            // This is the typical status used for a "skip" result.
            "NotSupported" => Ok(TestStatus::Skip),
            "ResourceError" => Ok(TestStatus::Fail),
            "InternalError" => Ok(TestStatus::Fail),
            "Crash" => Ok(TestStatus::Crash),
            "Timeout" => Ok(TestStatus::Timeout),
            // Treat waivers as "the test shouldn't be an error, but is a warning"
            "Waiver" => Ok(TestStatus::Warn),
            _ => anyhow::bail!("unknown dEQP status '{}'", input),
        }
    }
}

pub struct DeqpResultParser;

impl ResultParser for DeqpResultParser {
    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>> {
        lazy_static! {
            static ref TEST_RE: Regex = Regex::new("Test case '(.*)'..").unwrap();
            static ref STATUS_RE: Regex = Regex::new("^  (\\S*) \\(.*\\)").unwrap();
        }

        if let Some(cap) = TEST_RE.captures(line) {
            let name = cap[1].to_string();
            Ok(Some(ParserState::BeginTest(name)))
        } else if let Some(cap) = STATUS_RE.captures(line) {
            let status = &cap[1];

            /* One of the VK sparse tests emits some info lines about a NotSupported before actually saying NotSupported. */
            if status == "Info" {
                return Ok(None);
            }

            let status = TestStatus::from_deqp_str(&cap[1])?;
            Ok(Some(ParserState::EndTest(status)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaselistResult, TestResult};
    use std::time::Duration;

    fn parse_deqp_results(output: &mut &[u8]) -> Result<CaselistResult> {
        let parser = DeqpResultParser {};
        parser.parse(output)
    }

    fn result(name: &str, status: TestStatus) -> TestResult {
        TestResult {
            name: name.to_string(),
            status,
            duration: Duration::new(0, 0),
            subtests: vec![],
        }
    }

    #[test]
    fn parse_statuses() {
        let output = "
Writing test log into /home/anholt/TestResults.qpa
dEQP Core git-f442587f49bded2eec0c04fca6eb78d60ee83e8f (0xf442587f) starting..
  target implementation = 'Surfaceless'

Test case 'dEQP-GLES2.test.p'..
  Pass (Supported)

Test case 'dEQP-GLES2.test.f'..
  Fail (Found invalid pixel values)

Test case 'dEQP-GLES2.test.q'..
  QualityWarning (not so hot)

Test case 'dEQP-GLES2.test.c'..
  CompatibilityWarning (some bug)

Test case 'dEQP-GLES2.test.pend'..
  Pending (is there even a test that emits this)

Test case 'dEQP-GLES2.test.ns'..
  NotSupported (GL_EXT_tessellation_shader is not supported)

Test case 'dEQP-GLES2.test.re'..
  ResourceError (something missing)

Test case 'dEQP-GLES2.test.ie'..
  InternalError (whoops)

Test case 'dEQP-GLES2.test.cr'..
  Crash (boom)

Test case 'dEQP-GLES2.test.time'..
  Timeout (deqp watchdog)

Test case 'dEQP-GLES2.test.waive'..
  Waiver (it's fine)

DONE!

Test run totals:
  Passed:        6/6 (100.0%)
  Failed:        0/6 (0.0%)
  Not supported: 0/6 (0.0%)
  Warnings:      0/6 (0.0%)
  Waived:        0/6 (0.0%)";

        assert_eq!(
            parse_deqp_results(&mut output.as_bytes()).unwrap().results,
            vec![
                result("dEQP-GLES2.test.p", TestStatus::Pass),
                result("dEQP-GLES2.test.f", TestStatus::Fail),
                result("dEQP-GLES2.test.q", TestStatus::Warn),
                result("dEQP-GLES2.test.c", TestStatus::Warn),
                result("dEQP-GLES2.test.pend", TestStatus::Fail),
                result("dEQP-GLES2.test.ns", TestStatus::Skip),
                result("dEQP-GLES2.test.re", TestStatus::Fail),
                result("dEQP-GLES2.test.ie", TestStatus::Fail),
                result("dEQP-GLES2.test.cr", TestStatus::Crash),
                result("dEQP-GLES2.test.time", TestStatus::Timeout),
                result("dEQP-GLES2.test.waive", TestStatus::Warn),
            ]
        );
    }

    #[test]
    /// Test parsing a run that didn't produce any results, common when doing something like
    /// using a test list for the wrong deqp binary.
    fn parse_all_missing() {
        let output = "
dEQP Core git-f442587f49bded2eec0c04fca6eb78d60ee83e8f (0xf442587f) starting..
  target implementation = 'Surfaceless'

DONE!

Test run totals:
  Passed:        0/0 (0.0%)
  Failed:        0/0 (0.0%)
  Not supported: 0/0 (0.0%)
  Warnings:      0/0 (0.0%)
  Waived:        0/0 (0.0%)";

        assert_eq!(
            parse_deqp_results(&mut output.as_bytes()).unwrap().results,
            Vec::new()
        );
    }
    #[test]
    fn parse_crash() {
        let output = "
Writing test log into /home/anholt/TestResults.qpa
dEQP Core git-f442587f49bded2eec0c04fca6eb78d60ee83e8f (0xf442587f) starting..
  target implementation = 'Surfaceless'

Test case 'dEQP-GLES2.test.p'..
  Pass (Supported)

Test case 'dEQP-GLES2.test.c'..";

        assert_eq!(
            parse_deqp_results(&mut output.as_bytes()).unwrap().results,
            vec![
                result("dEQP-GLES2.test.p", TestStatus::Pass),
                result("dEQP-GLES2.test.c", TestStatus::Crash),
            ]
        );
    }

    #[test]
    fn parse_failure() {
        let output = "
Writing test log into /home/anholt/TestResults.qpa
dEQP Core git-f442587f49bded2eec0c04fca6eb78d60ee83e8f (0xf442587f) starting..
  target implementation = 'Surfaceless'

Test case 'dEQP-GLES2.test.p'..
  Pass (Supported)

Test case 'dEQP-GLES2.test.parsefail'..
  UnknownStatus (unknown)";

        assert!(parse_deqp_results(&mut output.as_bytes()).is_err());
    }

    #[test]
    fn parse_parens_in_detail() {
        let output = "
Writing test log into /home/anholt/TestResults.qpa
dEQP Core git-f442587f49bded2eec0c04fca6eb78d60ee83e8f (0xf442587f) starting..
  target implementation = 'Surfaceless'

Test case 'dEQP-GLES2.test.parens'..
  NotSupported (Test requires GL_MAX_VERTEX_SHADER_STORAGE_BLOCKS (0) >= 0)'..";

        assert_eq!(
            parse_deqp_results(&mut output.as_bytes()).unwrap().results,
            vec![result("dEQP-GLES2.test.parens", TestStatus::Skip),]
        );
    }

    #[test]
    fn parse_turnip_stdout_mixup() -> Result<()> {
        let output = include_bytes!("../resources/test/turnip-stdout-mixup.txt");
        let caselist_results = parse_deqp_results(&mut output.as_ref())?;
        let results = caselist_results.results;

        for result in &results {
            // Make sure parsing didn't mix up our stdout with some other lines.
            assert!(result.name.starts_with("dEQP-VK."));
        }

        assert_eq!(
            results[results.len() - 1].name,
            "dEQP-VK.compute.device_group.device_index"
        );
        assert_eq!(results[results.len() - 1].status, TestStatus::Crash);

        assert_eq!(results.len(), 347);

        Ok(())
    }

    #[test]
    fn parse_vk_sparse_info() -> Result<()> {
        let output = r#"
Test case 'dEQP-VK.api.buffer_memory_requirements.create_sparse_binding_sparse_residency_sparse_aliased.ext_mem_flags_included.method1.size_req_transfer_usage_bits'..
  Info (Create buffer with VK_BUFFER_CREATE_SPARSE_BINDING_BIT not supported by device at vktApiBufferMemoryRequirementsTests.cpp:353)
  Info (Create buffer with VK_BUFFER_CREATE_SPARSE_RESIDENCY_BIT not supported by device at vktApiBufferMemoryRequirementsTests.cpp:359)
  Info (Create buffer with VK_BUFFER_CREATE_SPARSE_ALIASED_BIT not supported by device at vktApiBufferMemoryRequirementsTests.cpp:365)
  NotSupported (One or more create buffer flags not supported by device at vktApiBufferMemoryRequirementsTests.cpp:377)"#;

        let caselist_results = parse_deqp_results(&mut output.as_bytes())?;
        let results = caselist_results.results;
        assert_eq!(results[0].status, TestStatus::Skip);

        Ok(())
    }
}
