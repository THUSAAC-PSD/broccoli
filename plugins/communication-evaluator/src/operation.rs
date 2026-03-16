use broccoli_server_sdk::types::{
    BuildEvalOpsInput, Channel, Environment, IOConfig, IOTarget, OperationTask, ResolvedLanguage,
    RunOptions, SessionFile, Step, StepCacheConfig,
};

use crate::config::{CommConfig, CommunicationMode, ManagerSourceEntry, SandboxConfig};

/// Build the communication operation for one test case.
pub fn build_operation(
    req: &BuildEvalOpsInput,
    contestant_lang: &ResolvedLanguage,
    manager_lang: &ResolvedLanguage,
    manager_files: &[ManagerSourceEntry],
    comm_config: &CommConfig,
    sandbox_config: &SandboxConfig,
) -> Result<Vec<OperationTask>, String> {
    let n = comm_config.num_processes as usize;
    if n == 0 {
        return Err("num_processes must be >= 1".to_string());
    }

    let time_limit_ms = u32::try_from(req.time_limit_ms)
        .map_err(|_| format!("Invalid time_limit_ms: {}", req.time_limit_ms))?;
    let time_limit_s = time_limit_ms as f64 / 1000.0;
    let memory_limit_kb = u32::try_from(req.memory_limit_kb)
        .map_err(|_| format!("Invalid memory_limit_kb: {}", req.memory_limit_kb))?;

    let mut channels = Vec::new();
    for i in 0..n {
        channels.push(Channel {
            name: format!("c{i}_to_m"),
            buffer_size: Some(8192),
        });
        channels.push(Channel {
            name: format!("m_to_c{i}"),
            buffer_size: Some(8192),
        });
    }

    let mut environments = Vec::new();

    let mut manager_files_in: Vec<(String, SessionFile)> = manager_files
        .iter()
        .map(|f| {
            (
                f.filename.clone(),
                SessionFile::Blob {
                    hash: f.hash.clone(),
                },
            )
        })
        .collect();
    manager_files_in.push((
        "input.txt".to_string(),
        SessionFile::Content {
            content: req.test_input.clone(),
        },
    ));

    environments.push(Environment {
        id: "manager_env".to_string(),
        files_in: manager_files_in,
    });

    let primary_source = req
        .solution_source
        .iter()
        .find(|s| s.filename == contestant_lang.source_filename)
        .or_else(|| req.solution_source.first())
        .ok_or("No contestant source file provided")?;

    for i in 0..n {
        let mut files_in: Vec<(String, SessionFile)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for source in &req.solution_source {
            if seen.insert(source.filename.clone()) {
                files_in.push((
                    source.filename.clone(),
                    SessionFile::Content {
                        content: source.content.clone(),
                    },
                ));
            }
        }
        if seen.insert(contestant_lang.source_filename.clone()) {
            files_in.push((
                contestant_lang.source_filename.clone(),
                SessionFile::Content {
                    content: primary_source.content.clone(),
                },
            ));
        }

        environments.push(Environment {
            id: format!("contestant_{i}"),
            files_in,
        });
    }

    let mut steps = Vec::new();

    if let Some(compile_cmd) = &manager_lang.compile_cmd {
        let manager_cache_inputs: Vec<String> =
            manager_files.iter().map(|f| f.filename.clone()).collect();
        steps.push(Step {
            id: "compile_manager".to_string(),
            env_ref: "manager_env".to_string(),
            argv: compile_cmd.clone(),
            conf: RunOptions {
                resource_limits: sandbox_config.compile_limits(),
                wait: true,
                env_rules: vec![],
                ..Default::default()
            },
            io: IOConfig {
                stdin: IOTarget::Null,
                stdout: IOTarget::Null,
                stderr: IOTarget::File {
                    path: "compile_stderr.txt".to_string(),
                },
            },
            collect: vec![
                manager_lang.binary_name.clone(),
                "compile_stderr.txt".to_string(),
            ],
            depends_on: vec![],
            cache: Some(StepCacheConfig {
                key_inputs: manager_cache_inputs,
                outputs: vec![manager_lang.binary_name.clone()],
            }),
        });
    }

    let contestant_compile_inputs: Vec<String> = {
        let mut inputs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for source in &req.solution_source {
            if seen.insert(source.filename.clone()) {
                inputs.push(source.filename.clone());
            }
        }
        if seen.insert(contestant_lang.source_filename.clone()) {
            inputs.push(contestant_lang.source_filename.clone());
        }
        inputs
    };

    for i in 0..n {
        if let Some(compile_cmd) = &contestant_lang.compile_cmd {
            steps.push(Step {
                id: format!("compile_contestant_{i}"),
                env_ref: format!("contestant_{i}"),
                argv: compile_cmd.clone(),
                conf: RunOptions {
                    resource_limits: sandbox_config.compile_limits(),
                    wait: true,
                    env_rules: vec![],
                    ..Default::default()
                },
                io: IOConfig {
                    stdin: IOTarget::Null,
                    stdout: IOTarget::Null,
                    stderr: IOTarget::File {
                        path: "compile_stderr.txt".to_string(),
                    },
                },
                collect: vec![
                    contestant_lang.binary_name.clone(),
                    "compile_stderr.txt".to_string(),
                ],
                depends_on: vec![],
                cache: Some(StepCacheConfig {
                    key_inputs: contestant_compile_inputs.clone(),
                    outputs: vec![contestant_lang.binary_name.clone()],
                }),
            });
        }
    }

    let mut all_compile_deps: Vec<String> = Vec::new();
    if manager_lang.compile_cmd.is_some() {
        all_compile_deps.push("compile_manager".to_string());
    }
    if contestant_lang.compile_cmd.is_some() {
        for i in 0..n {
            all_compile_deps.push(format!("compile_contestant_{i}"));
        }
    }

    let mut manager_argv = manager_lang.run_cmd.clone();
    for i in 0..n {
        manager_argv.push(format!("channels/c{i}_to_m"));
        manager_argv.push(format!("channels/m_to_c{i}"));
    }

    steps.push(Step {
        id: "run_manager".to_string(),
        env_ref: "manager_env".to_string(),
        argv: manager_argv,
        conf: RunOptions {
            resource_limits: sandbox_config.manager_limits(
                comm_config.manager_time_limit_s,
                comm_config.manager_memory_limit_kb,
            ),
            wait: true,
            env_rules: vec![],
            ..Default::default()
        },
        io: IOConfig {
            stdin: IOTarget::File {
                path: "input.txt".to_string(),
            },
            stdout: IOTarget::Inherit,
            stderr: IOTarget::Inherit,
        },
        collect: vec![],
        depends_on: all_compile_deps.clone(),
        cache: None,
    });

    for i in 0..n {
        let (io, argv) = match comm_config.communication_mode {
            CommunicationMode::Redirect => {
                let io = IOConfig {
                    stdin: IOTarget::Pipe {
                        name: format!("m_to_c{i}"),
                    },
                    stdout: IOTarget::Pipe {
                        name: format!("c{i}_to_m"),
                    },
                    stderr: IOTarget::Inherit,
                };
                let mut argv = contestant_lang.run_cmd.clone();
                argv.push(i.to_string());
                (io, argv)
            }
            CommunicationMode::FifoArgs => {
                let io = IOConfig {
                    stdin: IOTarget::Null,
                    stdout: IOTarget::Null,
                    stderr: IOTarget::Inherit,
                };
                let mut argv = contestant_lang.run_cmd.clone();
                argv.push(format!("channels/m_to_c{i}"));
                argv.push(format!("channels/c{i}_to_m"));
                argv.push(i.to_string());
                (io, argv)
            }
        };

        steps.push(Step {
            id: format!("run_contestant_{i}"),
            env_ref: format!("contestant_{i}"),
            argv,
            conf: RunOptions {
                resource_limits: sandbox_config.exec_limits(time_limit_s, memory_limit_kb),
                wait: true,
                env_rules: vec![],
                ..Default::default()
            },
            io,
            collect: vec![],
            depends_on: all_compile_deps.clone(),
            cache: None,
        });
    }

    Ok(vec![OperationTask {
        environments,
        tasks: steps,
        channels,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use broccoli_server_sdk::types::SourceFile;

    fn make_req() -> BuildEvalOpsInput {
        BuildEvalOpsInput {
            problem_id: 1,
            test_case_id: 42,
            solution_source: vec![SourceFile {
                filename: "main.cpp".to_string(),
                content: "int main() {}".to_string(),
            }],
            solution_language: "cpp".to_string(),
            time_limit_ms: 2000,
            memory_limit_kb: 262144,
            test_input: "5\n1 2 3 4 5\n".to_string(),
            expected_output: String::new(),
            checker_format: None,
            checker_config: None,
            checker_source: None,
        }
    }

    fn compiled_lang() -> ResolvedLanguage {
        ResolvedLanguage {
            compile_cmd: Some(vec![
                "/usr/bin/g++".to_string(),
                "-O2".to_string(),
                "solution.cpp".to_string(),
                "-o".to_string(),
                "solution".to_string(),
            ]),
            run_cmd: vec!["./solution".to_string()],
            source_filename: "solution.cpp".to_string(),
            binary_name: "solution".to_string(),
        }
    }

    fn manager_lang() -> ResolvedLanguage {
        ResolvedLanguage {
            compile_cmd: Some(vec![
                "/usr/bin/g++".to_string(),
                "-O2".to_string(),
                "manager.cpp".to_string(),
                "-o".to_string(),
                "manager".to_string(),
            ]),
            run_cmd: vec!["./manager".to_string()],
            source_filename: "manager.cpp".to_string(),
            binary_name: "manager".to_string(),
        }
    }

    fn mgr_files() -> Vec<ManagerSourceEntry> {
        vec![ManagerSourceEntry {
            filename: "manager.cpp".to_string(),
            hash: "abc123deadbeef".to_string(),
        }]
    }

    fn default_comm() -> CommConfig {
        CommConfig::default()
    }

    fn default_sandbox() -> SandboxConfig {
        SandboxConfig::default()
    }

    fn build(req: &BuildEvalOpsInput, comm: &CommConfig) -> Vec<OperationTask> {
        build_operation(
            req,
            &compiled_lang(),
            &manager_lang(),
            &mgr_files(),
            comm,
            &default_sandbox(),
        )
        .unwrap()
    }

    #[test]
    fn n1_redirect_produces_correct_structure() {
        let ops = build(&make_req(), &default_comm());

        assert_eq!(ops.len(), 1);
        let op = &ops[0];

        assert_eq!(op.channels.len(), 2);
        assert_eq!(op.channels[0].name, "c0_to_m");
        assert_eq!(op.channels[1].name, "m_to_c0");

        assert_eq!(op.environments.len(), 2);
        assert_eq!(op.environments[0].id, "manager_env");
        assert_eq!(op.environments[1].id, "contestant_0");

        assert_eq!(op.tasks.len(), 4);
        let step_ids: Vec<&str> = op.tasks.iter().map(|s| s.id.as_str()).collect();
        assert!(step_ids.contains(&"compile_manager"));
        assert!(step_ids.contains(&"compile_contestant_0"));
        assert!(step_ids.contains(&"run_manager"));
        assert!(step_ids.contains(&"run_contestant_0"));
    }

    #[test]
    fn n2_creates_four_channels_and_three_envs() {
        let mut comm = default_comm();
        comm.num_processes = 2;

        let ops = build(&make_req(), &comm);
        let op = &ops[0];
        assert_eq!(op.channels.len(), 4);
        assert_eq!(op.environments.len(), 3);
        assert_eq!(op.tasks.len(), 6);
    }

    #[test]
    fn manager_source_from_config_blob_hash() {
        let ops = build(&make_req(), &default_comm());
        let mgr_env = &ops[0].environments[0];

        let mgr_file = mgr_env
            .files_in
            .iter()
            .find(|(name, _)| name == "manager.cpp")
            .expect("manager source not in environment");
        match &mgr_file.1 {
            SessionFile::Blob { hash } => assert_eq!(hash, "abc123deadbeef"),
            _ => panic!("expected Blob for manager source"),
        }
    }

    #[test]
    fn multi_file_manager_mounts_all_files() {
        let files = vec![
            ManagerSourceEntry {
                filename: "manager.cpp".to_string(),
                hash: "hash_mgr".to_string(),
            },
            ManagerSourceEntry {
                filename: "manager.h".to_string(),
                hash: "hash_hdr".to_string(),
            },
        ];
        let ops = build_operation(
            &make_req(),
            &compiled_lang(),
            &manager_lang(),
            &files,
            &default_comm(),
            &default_sandbox(),
        )
        .unwrap();

        let mgr_env = &ops[0].environments[0];

        let cpp_file = mgr_env
            .files_in
            .iter()
            .find(|(name, _)| name == "manager.cpp")
            .expect("manager.cpp not in environment");
        match &cpp_file.1 {
            SessionFile::Blob { hash } => assert_eq!(hash, "hash_mgr"),
            _ => panic!("expected Blob for manager.cpp"),
        }

        let h_file = mgr_env
            .files_in
            .iter()
            .find(|(name, _)| name == "manager.h")
            .expect("manager.h not in environment");
        match &h_file.1 {
            SessionFile::Blob { hash } => assert_eq!(hash, "hash_hdr"),
            _ => panic!("expected Blob for manager.h"),
        }

        // Compile cache should include both filenames
        let compile_mgr = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "compile_manager")
            .unwrap();
        let cache = compile_mgr.cache.as_ref().unwrap();
        assert_eq!(
            cache.key_inputs,
            vec!["manager.cpp".to_string(), "manager.h".to_string()]
        );
    }

    #[test]
    fn manager_argv_includes_channel_paths() {
        let ops = build(&make_req(), &default_comm());
        let run_mgr = ops[0].tasks.iter().find(|s| s.id == "run_manager").unwrap();
        assert!(run_mgr.argv.contains(&"channels/c0_to_m".to_string()));
        assert!(run_mgr.argv.contains(&"channels/m_to_c0".to_string()));
    }

    #[test]
    fn redirect_mode_uses_pipe_io() {
        let ops = build(&make_req(), &default_comm());
        let run_c0 = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "run_contestant_0")
            .unwrap();

        match &run_c0.io.stdin {
            IOTarget::Pipe { name } => assert_eq!(name, "m_to_c0"),
            _ => panic!("expected Pipe for stdin in redirect mode"),
        }
        match &run_c0.io.stdout {
            IOTarget::Pipe { name } => assert_eq!(name, "c0_to_m"),
            _ => panic!("expected Pipe for stdout in redirect mode"),
        }
    }

    #[test]
    fn fifo_args_mode_uses_null_io_and_argv_paths() {
        let mut comm = default_comm();
        comm.communication_mode = CommunicationMode::FifoArgs;

        let ops = build(&make_req(), &comm);
        let run_c0 = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "run_contestant_0")
            .unwrap();

        assert!(matches!(run_c0.io.stdin, IOTarget::Null));
        assert!(matches!(run_c0.io.stdout, IOTarget::Null));
        assert!(run_c0.argv.contains(&"channels/m_to_c0".to_string()));
        assert!(run_c0.argv.contains(&"channels/c0_to_m".to_string()));
    }

    #[test]
    fn contestant_process_index_is_last_argv() {
        let ops = build(&make_req(), &default_comm());
        let run_c0 = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "run_contestant_0")
            .unwrap();
        assert_eq!(run_c0.argv.last().unwrap(), "0");
    }

    #[test]
    fn manager_reads_test_input_from_stdin() {
        let ops = build(&make_req(), &default_comm());
        let run_mgr = ops[0].tasks.iter().find(|s| s.id == "run_manager").unwrap();
        match &run_mgr.io.stdin {
            IOTarget::File { path } => assert_eq!(path, "input.txt"),
            _ => panic!("expected File for manager stdin"),
        }
    }

    #[test]
    fn all_run_steps_depend_on_all_compile_steps() {
        let ops = build(&make_req(), &default_comm());

        let run_mgr = ops[0].tasks.iter().find(|s| s.id == "run_manager").unwrap();
        assert!(run_mgr.depends_on.contains(&"compile_manager".to_string()));
        assert!(
            run_mgr
                .depends_on
                .contains(&"compile_contestant_0".to_string())
        );

        let run_c0 = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "run_contestant_0")
            .unwrap();
        assert!(
            run_c0
                .depends_on
                .contains(&"compile_contestant_0".to_string())
        );
        assert!(run_c0.depends_on.contains(&"compile_manager".to_string()));
    }

    #[test]
    fn run_manager_and_contestants_do_not_depend_on_each_other() {
        let ops = build(&make_req(), &default_comm());

        let run_mgr = ops[0].tasks.iter().find(|s| s.id == "run_manager").unwrap();
        let run_c0 = ops[0]
            .tasks
            .iter()
            .find(|s| s.id == "run_contestant_0")
            .unwrap();

        assert!(!run_mgr.depends_on.contains(&"run_contestant_0".to_string()));
        assert!(!run_c0.depends_on.contains(&"run_manager".to_string()));
    }

    #[test]
    fn manager_stdout_stderr_are_inherit() {
        let ops = build(&make_req(), &default_comm());
        let run_mgr = ops[0].tasks.iter().find(|s| s.id == "run_manager").unwrap();
        assert!(matches!(run_mgr.io.stdout, IOTarget::Inherit));
        assert!(matches!(run_mgr.io.stderr, IOTarget::Inherit));
    }

    #[test]
    fn stubs_in_solution_source_appear_in_contestant_env() {
        let mut req = make_req();
        // Server has already merged stubs into solution_source
        req.solution_source.push(SourceFile {
            filename: "stub.cpp".to_string(),
            content: "void stub_fn() {}".to_string(),
        });

        let ops = build(&req, &default_comm());
        let c0_env = ops[0]
            .environments
            .iter()
            .find(|e| e.id == "contestant_0")
            .unwrap();
        assert!(
            c0_env.files_in.iter().any(|(name, _)| name == "stub.cpp"),
            "stub should be in contestant environment"
        );
    }
}
