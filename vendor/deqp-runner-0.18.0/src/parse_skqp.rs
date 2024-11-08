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

use anyhow::Result;
use log::*;
use regex::Regex;
use std::fmt::Display;

use crate::parse::{ParserState, ResultParser};
use crate::{TestCase, TestStatus};

impl TestStatus {
    // Parses the status name from piglit's output.
    fn from_skqp_str(input: &str) -> TestStatus {
        match input {
            "passed" => TestStatus::Pass,
            "Passed" => TestStatus::Pass,
            "FAILED" => TestStatus::Fail,
            "ERROR" => TestStatus::Crash,
            _ => {
                error!("unknown skqp status '{}'", input);
                TestStatus::Crash
            }
        }
    }
}

pub struct SkqpResultParser {
    test_name: String,
    default_status: TestStatus,
}

impl SkqpResultParser {
    pub fn new(name: &str) -> SkqpResultParser {
        SkqpResultParser {
            test_name: String::from(name),
            default_status: TestStatus::Skip,
        }
    }
}

impl ResultParser for SkqpResultParser {
    fn initialize(&mut self) -> Option<ParserState> {
        Some(ParserState::BeginTest(self.test_name.to_owned()))
    }

    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>> {
        lazy_static! {
            static ref STATUS_RE: Regex =
                Regex::new(r#".*(passed|Passed|ERROR|FAILED).*: .*"#).unwrap();
        }

        // If we get anything on stdout, then the test should have started and
        // we expect a status.  If no stdout happens, then it's an unsupported
        // testcase (such as when a feature is disabled in the build)
        if !line.is_empty() {
            self.default_status = TestStatus::Crash
        }

        if let Some(cap) = STATUS_RE.captures(line) {
            let status = TestStatus::from_skqp_str(&cap[1]);
            Ok(Some(ParserState::EndTest(status)))
        } else {
            Ok(None)
        }
    }

    fn default_test_status(&self) -> TestStatus {
        self.default_status
    }
}

fn skqp_format_test_name(backend: &str, test: &str) -> String {
    format!("{}_{}", backend, test.replace(',', "@"))
}

pub enum SkqpTestFile {
    RenderTest,
    UnitTest,
}

impl Display for SkqpTestFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SkqpTestFile::RenderTest => "rendertests.txt",
                SkqpTestFile::UnitTest => "unittests.txt",
            }
        )
    }
}

pub fn skqp_parse_testlist_file(file_content: &str) -> Result<Vec<&str>> {
    let tests: Vec<&str> = file_content
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
        .map(|x| x.split(',').next().unwrap())
        .collect();
    Ok(tests)
}

pub fn skqp_parse_testcase(test: &str, backends: &[&str]) -> Result<Vec<crate::TestCase>> {
    let mut tests: Vec<TestCase> = Vec::new();
    for backend in backends {
        tests.push(TestCase::Named(skqp_format_test_name(backend, test)));
    }
    Ok(tests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skqp_testlist_unittest() {
        let file_content = "
SpecialSurface_Gpu1
SurfaceAttachStencil_Gpu
SurfaceBackendHandleAccessIDs_Gpu
SurfaceBackendSurfaceAccessCopyOnWrite_Gpu
SurfaceBudget
SurfaceCRBug263329_Gpu
SurfaceCanvasPeek_Gpu
SurfaceClear_Gpu
SurfaceCopyOnWrite_Gpu
SurfaceEmpty_Gpu
SurfaceNoCanvas_Gpu
SurfacePartialDraw_Gpu
SurfaceSemaphores
SurfaceSnapshotAlphaType_Gpu
SurfaceWriteableAfterSnapshotRelease_Gpu
";
        let results = skqp_parse_testlist_file(file_content).unwrap();
        assert_eq!(
            results,
            vec![
                "SpecialSurface_Gpu1",
                "SurfaceAttachStencil_Gpu",
                "SurfaceBackendHandleAccessIDs_Gpu",
                "SurfaceBackendSurfaceAccessCopyOnWrite_Gpu",
                "SurfaceBudget",
                "SurfaceCRBug263329_Gpu",
                "SurfaceCanvasPeek_Gpu",
                "SurfaceClear_Gpu",
                "SurfaceCopyOnWrite_Gpu",
                "SurfaceEmpty_Gpu",
                "SurfaceNoCanvas_Gpu",
                "SurfacePartialDraw_Gpu",
                "SurfaceSemaphores",
                "SurfaceSnapshotAlphaType_Gpu",
                "SurfaceWriteableAfterSnapshotRelease_Gpu"
            ],
        );
    }

    #[test]
    fn parse_skqp_testlist_rendertest() {
        let file_content = "
3dgm,-1
3x3bitmaprect,0
AnimCodecPlayer,0
BlurDrawImage,-1
CubicStroke,-1
OverStroke,-1
PlusMergesAA,0
aaclip,-1
aarectmodes,-1
aaxfermodes,-1
addarc,-1
addarc_meas,-1
all_bitmap_configs,0
all_variants_8888,0
alpha_image,0
";
        let results = skqp_parse_testlist_file(file_content).unwrap();
        assert_eq!(
            results,
            vec![
                "3dgm",
                "3x3bitmaprect",
                "AnimCodecPlayer",
                "BlurDrawImage",
                "CubicStroke",
                "OverStroke",
                "PlusMergesAA",
                "aaclip",
                "aarectmodes",
                "aaxfermodes",
                "addarc",
                "addarc_meas",
                "all_bitmap_configs",
                "all_variants_8888",
                "alpha_image"
            ],
        );
    }

    #[test]
    fn parse_skqp_testcase() {
        let test = "AnimCodecPlayer";
        let backends = vec!["gl", "gles", "vk"];
        let results = skqp_parse_testcase(test, &backends).unwrap();
        assert_eq!(
            results,
            vec![
                TestCase::Named("gl_AnimCodecPlayer".to_string(),),
                TestCase::Named("gles_AnimCodecPlayer".to_string(),),
                TestCase::Named("vk_AnimCodecPlayer".to_string(),),
            ]
        );
    }

    fn parse_stdout_to_status(name: &str, stdout: &str) -> Result<TestStatus> {
        let parser = SkqpResultParser::new(name);
        let results = parser
            .parse_with_timer(stdout.as_bytes(), None)
            .unwrap()
            .results;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, name);

        Ok(results[0].status)
    }

    #[test]
    fn parse_supported_testcase() {
        let stdout = r#"
Starting: gl_3x3bitmaprect
Passed:   gl_3x3bitmaprect
"#;

        assert_eq!(
            parse_stdout_to_status("gl_3x3bitmaprect", stdout).unwrap(),
            TestStatus::Pass
        );
    }

    #[test]
    fn parse_unsupported_testcase() {
        assert_eq!(
            parse_stdout_to_status("test", "").unwrap(),
            TestStatus::Skip
        );
    }
}
