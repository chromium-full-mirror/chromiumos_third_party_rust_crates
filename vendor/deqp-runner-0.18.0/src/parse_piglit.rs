// Copyright (c) 2021 Advanced Micro Devices, Inc.
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
use flate2::read::GzDecoder;
use log::*;
use regex::Regex;
use roxmltree::Document;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use crate::parse::{ParserState, ResultParser};
use crate::{BinaryTest, TestCase, TestStatus};

impl TestStatus {
    // Parses the status name from piglit's output.
    fn from_piglit_str(input: &str) -> TestStatus {
        match input {
            "pass" => TestStatus::Pass,
            "fail" => TestStatus::Fail,
            "warn" => TestStatus::Warn,
            "crash" => TestStatus::Crash,
            "skip" => TestStatus::Skip,
            "timeout" => TestStatus::Timeout,
            _ => {
                error!("unknown piglit status '{}'", input);
                TestStatus::Crash
            }
        }
    }
}

pub struct PiglitResultParser {
    test_name: String,
}

impl PiglitResultParser {
    pub fn new(name: &str) -> PiglitResultParser {
        PiglitResultParser {
            test_name: piglit_sanitize_test_name(name),
        }
    }
}

impl ResultParser for PiglitResultParser {
    fn initialize(&mut self) -> Option<ParserState> {
        Some(ParserState::BeginTest(self.test_name.to_owned()))
    }

    fn parse_line(&mut self, line: &str) -> Result<Option<ParserState>> {
        lazy_static! {
            static ref STATUS_RE: Regex = Regex::new(r#"PIGLIT: \{"result": "(.*)" \}"#).unwrap();
            static ref SUBTEST_RE: Regex =
                Regex::new(r#"PIGLIT: \{"subtest": *\{"(.*)" *: *"(.*)"\}\}"#).unwrap();
        }

        if let Some(cap) = STATUS_RE.captures(line) {
            let status = TestStatus::from_piglit_str(&cap[1]);
            Ok(Some(ParserState::EndTest(status)))
        } else if let Some(cap) = SUBTEST_RE.captures(line) {
            let name = piglit_sanitize_test_name(&cap[1]);
            let status = TestStatus::from_piglit_str(&cap[2]);
            Ok(Some(ParserState::SubTest(name, status)))
        } else {
            Ok(None)
        }
    }
}

pub fn read_profile_file(
    piglit_folder: &std::path::Path,
    profile: &str,
    process_isolation: bool,
) -> Result<String> {
    // TODO: Don't read no_isolation version: deqp-runner is the one dispatching
    // multiple tests to a single runner. But this requires parsing the shader_test file
    // to determine which ones can run together (see piglit's shader_test.py)
    if !process_isolation {
        let path = piglit_folder.join(Path::new(profile).with_extension("no_isolation.meta.xml"));
        if path.exists() {
            info!("... using {:?}", &path);
            return std::fs::read_to_string(&path).with_context(|| format!("reading {:?}", path));
        }
    }

    {
        let path = piglit_folder.join(Path::new(profile).with_extension("meta.xml"));
        if path.exists() {
            info!("... using {:?}", path);
            return std::fs::read_to_string(&path).with_context(|| format!("reading {:?}", path));
        }
    }

    /* try .no_isolation.xml.gz first */
    if !process_isolation {
        let path = piglit_folder.join(Path::new(profile).with_extension("no_isolation.xml.gz"));
        if path.exists() {
            info!("... using {:?}", path);
            let file = File::open(&path).with_context(|| format!("opening {:?}", path))?;
            let mut s = String::new();
            GzDecoder::new(file)
                .read_to_string(&mut s)
                .with_context(|| format!("reading {:?}", path))?;
            return Ok(s);
        }
    }

    /* then try .no_isolation.xml */
    if !process_isolation {
        let path = piglit_folder.join(Path::new(profile).with_extension("no_isolation.xml"));
        if path.exists() {
            info!("... using {:?}", path);
            return std::fs::read_to_string(&path).with_context(|| format!("reading {:?}", path));
        }
    }

    /* then try .xml.gz */
    {
        let path = piglit_folder.join(Path::new(profile).with_extension("xml.gz"));
        if path.exists() {
            info!("... using {:?}", path);
            let file = File::open(&path).with_context(|| format!("opening {:?}", path))?;
            let mut s = String::new();
            GzDecoder::new(file)
                .read_to_string(&mut s)
                .with_context(|| format!("reading {:?}", path))?;
            return Ok(s);
        }
    }

    {
        let path = piglit_folder.join(Path::new(profile).with_extension("xml"));
        std::fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))
    }
}

fn get_option_value(node: &roxmltree::Node, name: &str) -> Result<String> {
    let mut children = node.children().filter(|x| x.has_tag_name("option"));
    let option_node = children
        .find(|x| x.attribute("name") == Some(name))
        .with_context(|| format!("Getting option {}", name))?;

    if children.any(|x| x.attribute("name") == Some(name)) {
        anyhow::bail!("More than one option named {}", name);
    }

    Ok(option_node
        .attribute("value")
        .with_context(|| format!("getting option {} value", name))?
        .to_string())
}

fn parse_piglit_command(
    test_name: &str,
    test_type: &str,
    run_concurrent: bool,
    command: &str,
) -> Result<TestCase> {
    let len = command.len();
    if len < 2 {
        anyhow::bail!("command length {} too short", len);
    }
    // The slice strips out the "[]" wrapping the python array dump encoded as
    // an XML string, then we strip off the single quotes around each array element
    // separated by ','.  This still feels fragile.
    let mut all = command[1..(len - 1)]
        .split(',')
        .map(|arg| arg.replace('\'', ""));
    let binary = all.next().context("Getting binary")?;
    let mut args: Vec<String> = all
        .map(|arg| arg.trim().to_string())
        .filter(|arg| !arg.is_empty())
        .map(|arg| {
            if test_type == "gl_builtin" {
                format!("tests/{}", arg)
            } else {
                arg
            }
        })
        .collect();

    match test_type {
        "glsl_parser" | "cl" => {}
        _ => {
            args.push("-auto".to_string());
            if run_concurrent {
                args.push("-fbo".to_string());
            }
        }
    }

    Ok(BinaryTest {
        name: test_name.to_string(),
        binary,
        args,
    }
    .into())
}

/* Replace ',' to avoid conflicts in the csv file */
fn piglit_sanitize_test_name(test: &str) -> String {
    test.replace(',', "-")
}

pub fn parse_piglit_test(tests: &mut Vec<TestCase>, test: &roxmltree::Node) -> Result<()> {
    let test_name = piglit_sanitize_test_name(test.attribute("name").context("getting test name")?);

    match test.attribute("type") {
        // multi_shader implements the same feature as deqp-runner: grouping test runs in a
        // single process.
        Some("multi_shader") => {
            let nodes: Vec<roxmltree::Node> = test
                .children()
                .filter(|n| n.attribute("name") == Some("files"))
                .collect();
            if !nodes.is_empty() {
                let content: String = nodes[0]
                    .attribute("value")
                    .unwrap()
                    .chars()
                    .filter(|c| !r#"[]'"#.contains(*c))
                    .collect();
                let all: Vec<&str> = content.split(',').collect();

                let mut args = Vec::new();

                for a in all {
                    let m = a.trim();
                    if !m.is_empty() {
                        args.push(m.to_string());
                    }
                }

                let mut remaining = args.len();
                let mut i = 0u32;
                while remaining != 0 {
                    let group_len = usize::min(100, remaining);
                    remaining -= group_len;

                    let mut a = args.split_off(remaining);
                    a.push("-auto".to_string());
                    a.push("-fbo".to_string());

                    tests.push(
                        BinaryTest {
                            name: format!("{}|{}", test_name, i),
                            binary: "shader_runner".to_string(),
                            args: a,
                        }
                        .into(),
                    );

                    i += 1;
                }
            }
        }

        Some("asm_parser") => {
            tests.push(
                BinaryTest {
                    name: test_name,
                    binary: "asmparsertest".to_string(),
                    args: vec![
                        get_option_value(test, "type_")?.replace('\'', ""),
                        get_option_value(test, "filename")?.replace('\'', ""),
                    ],
                }
                .into(),
            );
        }

        Some("cl") => {
            let command = get_option_value(test, "command").context("parsing command")?;
            tests.push(parse_piglit_command(&test_name, "cl", false, &command)?);
        }

        Some("cl_prog") => {
            tests.push(
                BinaryTest {
                    name: test_name,
                    binary: "cl-program-tester".to_string(),
                    args: vec![get_option_value(test, "filename")?.replace('\'', "")],
                }
                .into(),
            );
        }
        Some(test_type) => {
            let command = get_option_value(test, "command").context("parsing command")?;

            let run_concurrent = match get_option_value(test, "run_concurrent")
                .context("parsing concurrent")?
                .as_str()
            {
                "True" => true,
                "False" => false,
                x => anyhow::bail!("Unknown run_concurrent value {}", x),
            };

            tests.push(parse_piglit_command(
                &test_name,
                test_type,
                run_concurrent,
                &command,
            )?);
        }

        None => anyhow::bail!("No test type specified for {}", test_name),
    }
    Ok(())
}

pub fn parse_piglit_xml_testlist(
    folder: &Path,
    file_content: &str,
    process_isolation: bool,
) -> Result<Vec<crate::TestCase>> {
    let doc = Document::parse(file_content).context("reading caselist")?;

    let mut tests = Vec::new();

    /* meta profile */
    for test in doc.descendants().filter(|n| n.has_tag_name("Profile")) {
        if let Some(name) = test.text() {
            info!("Found subprofile: {:?}", name);
            let content = read_profile_file(folder, name, process_isolation)?;
            for t in parse_piglit_xml_testlist(folder, &content, process_isolation)? {
                tests.push(t);
            }
        }
    }

    for test in doc.descendants().filter(|n| n.has_tag_name("Test")) {
        parse_piglit_test(&mut tests, &test)
            .with_context(|| format!("parsing test node: {:?}", &test))?;
    }

    Ok(tests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaselistResult, TestResult};
    use std::path::PathBuf;
    use std::time::Duration;

    fn parse_immediate_xml(xml: &str) -> Result<Vec<TestCase>> {
        let dummy_path = PathBuf::from(".");
        parse_piglit_xml_testlist(&dummy_path, xml, false)
    }

    #[test]
    fn parse_command_and_args() {
        let xml = r#"
        <Test type="gl" name="fast_color_clear@fcc-read-after-clear blit rb">
        <option name="command" value="['fcc-read-after-clear', 'blit', 'rb']" />
        <option name="run_concurrent" value="True" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "fast_color_clear@fcc-read-after-clear blit rb".to_string(),
                binary: "fcc-read-after-clear".to_string(),
                args: vec![
                    "blit".to_string(),
                    "rb".to_string(),
                    "-auto".to_string(),
                    "-fbo".to_string(),
                ],
            }
            .into()
        );
    }

    #[test]
    fn parse_asmparsertest() {
        let xml = r#"
        <Test type="asm_parser" name="asmparsertest@arbfp1.0@cos-03.txt">
        <option name="type_" value="'ARBfp1.0'" />
        <option name="filename" value="'tests/asmparsertest/shaders/ARBfp1.0/cos-03.txt'" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "asmparsertest@arbfp1.0@cos-03.txt".to_string(),
                binary: "asmparsertest".to_string(),
                args: vec![
                    "ARBfp1.0".to_string(),
                    "tests/asmparsertest/shaders/ARBfp1.0/cos-03.txt".to_string()
                ],
            }
            .into()
        );
    }

    #[test]
    fn parse_glslparsertest() {
        let xml = r#"
        <Test type="glsl_parser" name="spec@ext_clip_cull_distance@preprocessor@disabled-defined-es.comp">
        <option name="shader_version" value="3.1" />
        <option name="api" value="'gles3'" />
        <option name="command" value="['glslparsertest_gles2', 'generated_tests/spec/ext_clip_cull_distance/preprocessor/disabled-defined-es.comp', 'pass', '3.10 es', '!GL_EXT_clip_cull_distance']" />
        <option name="run_concurrent" value="True" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "spec@ext_clip_cull_distance@preprocessor@disabled-defined-es.comp"
                    .to_string(),
                binary: "glslparsertest_gles2".to_string(),
                args: vec![
                    "generated_tests/spec/ext_clip_cull_distance/preprocessor/disabled-defined-es.comp".to_string(),
                    "pass".to_string(), "3.10 es".to_string(), "!GL_EXT_clip_cull_distance".to_string()
                ],
            }.into()
        );
    }

    #[test]
    fn parse_glx_test() {
        // This test must be run without -fbo, or it fails.
        let xml = r#"
        <Test type="gl" name="glx@glx-query-drawable-glxbaddrawable">
        <option name="require_platforms" value="['glx', 'mixed_glx_egl']" />
        <option name="command" value="['glx-query-drawable', '--bad-drawable']" />
        <option name="run_concurrent" value="False" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "glx@glx-query-drawable-glxbaddrawable".to_string(),
                binary: "glx-query-drawable".to_string(),
                args: vec!["--bad-drawable".to_string(), "-auto".to_string()],
            }
            .into()
        );
    }

    #[test]
    fn parse_command_with_brackets() -> Result<()> {
        let test = parse_piglit_command(
            "test",
            "gl",
            true,
            r#"['ext_transform_feedback-output-type', 'vec4', '[2]']"#,
        )?;

        assert_eq!(
            test,
            BinaryTest {
                name: "test".to_string(),
                binary: "ext_transform_feedback-output-type".to_string(),
                args: vec!(
                    "vec4".to_string(),
                    "[2]".to_string(),
                    "-auto".to_string(),
                    "-fbo".to_string(),
                )
            }
            .into()
        );

        Ok(())
    }

    fn output_as_lines(output: &str) -> Vec<String> {
        output.lines().map(|x| x.to_string()).collect()
    }

    fn parse_piglit_results<B: Read>(output: B) -> CaselistResult {
        let parser = PiglitResultParser::new("test");
        parser.parse(output).expect("Parser error")
    }

    fn result(status: TestStatus, subtests: Vec<TestResult>, orig_output: &str) -> CaselistResult {
        let result = TestResult {
            name: "test".to_owned(),
            status,
            duration: Duration::new(0, 0),
            subtests,
        };

        CaselistResult {
            results: vec![result],
            stdout: output_as_lines(orig_output),
        }
    }

    #[test]
    fn parse_statuses() {
        let output = "
PIGLIT: {\"result\": \"pass\" }";

        assert_eq!(
            parse_piglit_results(&mut output.as_bytes()),
            result(TestStatus::Pass, vec![], output),
        );
    }

    #[test]
    fn parse_subtests() {
        let output = "
PIGLIT: {\"enumerate subtests\": [\"Check valid integer border color values\", \"Check invalid integer border color values\", \"Check valid float border color values\", \"Check invalid float border color values\"]}
PIGLIT: {\"subtest\": {\"Check valid integer border color values\" : \"pass\"}}
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
PIGLIT: {\"subtest\": {\"Check invalid integer border color values\" : \"fail\"}}
PIGLIT: {\"subtest\": {\"Check valid float border color values\" : \"skip\"}}
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
Mesa: User error: GL_INVALID_OPERATION in glGetTextureSamplerHandleARB(invalid border color)
PIGLIT: {\"subtest\": {\"Check invalid float border color values\" : \"warn\"}}
PIGLIT: {\"result\": \"pass\" }";

        let subtests = vec![
            TestResult {
                name: "Check valid integer border color values".to_owned(),
                status: TestStatus::Pass,
                duration: Duration::new(0, 0),
                subtests: Vec::new(),
            },
            TestResult {
                name: "Check invalid integer border color values".to_owned(),
                status: TestStatus::Fail,
                duration: Duration::new(0, 0),
                subtests: Vec::new(),
            },
            TestResult {
                name: "Check valid float border color values".to_owned(),
                status: TestStatus::Skip,
                duration: Duration::new(0, 0),
                subtests: Vec::new(),
            },
            TestResult {
                name: "Check invalid float border color values".to_owned(),
                status: TestStatus::Warn,
                duration: Duration::new(0, 0),
                subtests: Vec::new(),
            },
        ];

        assert_eq!(
            parse_piglit_results(output.as_bytes()),
            result(TestStatus::Pass, subtests, output)
        );
    }

    #[test]
    fn parse_crash() {
        let output = r#"
        PIGLIT: {"subtest": {"Vertex shader/control memory barrier test/modulus=1" : "pass"}}
        PIGLIT: {"subtest": {"Tessellation control shader/control memory barrier test/modulus=1" : "pass"}}"#;

        let subtests = vec![
            TestResult {
                name: "Vertex shader/control memory barrier test/modulus=1".to_owned(),
                status: TestStatus::Pass,
                duration: Duration::new(0, 0),
                subtests: vec![],
            },
            TestResult {
                name: "Tessellation control shader/control memory barrier test/modulus=1"
                    .to_owned(),
                status: TestStatus::Pass,
                duration: Duration::new(0, 0),
                subtests: vec![],
            },
        ];

        assert_eq!(
            parse_piglit_results(output.as_bytes()),
            result(TestStatus::Crash, subtests, output)
        );
    }

    #[test]
    fn parse_shader_runner_subtests() {
        let output = r#"
PIGLIT TEST: 1 - glsl-fs-swizzle-1
PIGLIT TEST: 1 - glsl-fs-swizzle-1
PIGLIT: {"subtest": {"glsl-fs-swizzle-1" : "pass"}}
PIGLIT TEST: 2 - vs-sign-neg
PIGLIT TEST: 2 - vs-sign-neg
PIGLIT: {"subtest": {"vs-sign-neg" : "pass"}}
"#;

        let subtests = vec![
            TestResult {
                name: "glsl-fs-swizzle-1".to_owned(),
                status: TestStatus::Pass,
                duration: Duration::new(0, 0),
                subtests: vec![],
            },
            TestResult {
                name: "vs-sign-neg".to_owned(),
                status: TestStatus::Pass,
                duration: Duration::new(0, 0),
                subtests: vec![],
            },
        ];

        assert_eq!(
            parse_piglit_results(output.as_bytes()),
            result(TestStatus::Crash, subtests, output)
        );
    }

    #[test]
    fn parse_cl() {
        let xml = r#"
        <Test type="cl" name="api@clgeteventinfo">
        <option name="command" value="['cl-api-get-event-info']" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "api@clgeteventinfo".to_string(),
                binary: "cl-api-get-event-info".to_string(),
                args: vec![],
            }
            .into()
        );
    }

    #[test]
    fn parse_cl_prog() {
        let xml = r#"
        <Test type="cl_prog" name="program@build@define-gentype">
        <option name="filename" value="'tests/cl/program/build/define-GENTYPE.cl'" />
        </Test>"#;

        assert_eq!(
            parse_immediate_xml(xml).unwrap()[0],
            BinaryTest {
                name: "program@build@define-gentype".to_string(),
                binary: "cl-program-tester".to_string(),
                args: vec!["tests/cl/program/build/define-GENTYPE.cl".to_string()],
            }
            .into()
        );
    }
}
