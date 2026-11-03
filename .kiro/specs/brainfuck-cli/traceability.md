# brainfuck-cli 受け入れ基準のトレーサビリティ一覧

タスク 7.1 の確認結果として作成。各受け入れ基準(`requirements.md` の全項目)を、対応する単体テスト(`src/` 内の `#[cfg(test)]`)または結合テスト(`tests/*.rs`)へ対応付ける。

- 対象: `crates/brainfuck-cli`(ブランチ `feat/brainfuck-cli`)
- 記号: **対応** = 対応するテストが存在する / **部分** = テストはあるが一部が未検証、または自動テストでは検証できない / **未対応** = 対応するテストが無い
- テスト関数名は `ファイル::関数名` で示す。`src/` の単体テストは `src/<file>.rs::`、結合テストは `tests/<file>.rs::`。

## 要件 1: コマンドライン引数の受理と検証

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 1.1 パス 1 個で実行 | `tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr`、`src/app.rs::success_writes_output_and_leaves_stderr_empty` | 対応 |
| 1.2 5 個のオプションを受理 | `src/args.rs::all_options_with_separate_values`、`src/args.rs::default_max_steps_is_ten_million` | 対応 |
| 1.3 `--opt value` と `--opt=value` を同義に扱う | `src/args.rs::equals_form_is_same_as_separate_form`、`tests/args.rs::equals_form_and_separate_form_mean_the_same_for_input`、`tests/limits.rs::max_steps_equals_form_is_accepted` | 対応 |
| 1.4 オプションをパスの前後どちらに置いても同義 | `src/args.rs::options_may_precede_or_follow_program`、`tests/args.rs::input_option_before_or_after_program_path_means_the_same`、`tests/args.rs::max_steps_before_or_after_program_path_means_the_same` | 対応 |
| 1.5 パス欠落は 2、標準出力空、案内を表示 | `src/args.rs::missing_program_is_error`、`src/app.rs::missing_program_is_usage_error_with_empty_stdout`、`tests/args.rs::missing_program_path_is_usage_error`、`tests/args.rs::only_flags_without_program_path_is_usage_error`、`tests/skeleton.rs::binary_without_arguments_is_usage_error_with_empty_stdout` | 対応 |
| 1.6 パスが 2 個以上は 2 | `src/args.rs::extra_program_is_error`、`tests/args.rs::two_program_paths_are_usage_error` | 対応 |
| 1.7 未知オプションは 2 | `src/args.rs::unknown_option_is_error`、`tests/args.rs::unknown_option_is_usage_error`、`src/app.rs::unknown_option_diagnostic_does_not_echo_the_value` | 対応 |
| 1.8 値の欠落は 2 | `src/args.rs::missing_value_is_error`、`src/args.rs::empty_input_path_is_missing_value`、`tests/args.rs::input_option_without_value_is_usage_error`、`tests/args.rs::input_option_with_empty_inline_value_is_usage_error`、`tests/args.rs::max_steps_option_without_value_is_usage_error` | 対応 |
| 1.9 `--input` / `--max-steps` の重複は 2 | `src/args.rs::duplicate_value_option_is_error`、`tests/args.rs::duplicate_input_option_is_usage_error`、`tests/args.rs::duplicate_max_steps_option_is_usage_error` | 対応 |
| 1.10 引数検証失敗時はファイル読込・実行をしない | `tests/args.rs::argument_errors_do_not_read_program_input_or_stdin`、`src/args.rs::parsing_never_touches_the_filesystem_and_is_deterministic`、`tests/limits.rs::dump_state_is_not_printed_on_argument_error` | 対応 |
| 1.11 値なしオプションの重複は 1 回と同義 | `src/args.rs::duplicate_flags_are_idempotent`、`tests/args.rs::repeated_dump_state_behaves_like_single_flag`、`tests/args.rs::repeated_no_step_limit_behaves_like_single_flag` | 対応 |

## 要件 2: 使用方法の表示(`--help`)

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 2.1 使用方法を標準エラーへ、終了コード 0 | `src/app.rs::help_writes_usage_to_stderr_and_exits_zero`、`tests/args.rs::help_writes_full_usage_to_stderr_and_exits_zero` | 対応 |
| 2.2 ファイル・標準入力を読まず実行しない | `src/app.rs::help_does_not_read_the_program_file`、`tests/args.rs::help_does_not_read_program_input_or_stdin` | 対応 |
| 2.3 `--help` を引数エラーより優先 | `src/args.rs::help_takes_precedence_over_other_errors`、`tests/args.rs::help_takes_precedence_over_other_argument_errors` | 対応 |
| 2.4 使用方法の内容(オプション、既定値、終了コード) | `src/args.rs::usage_mentions_all_options_default_and_exit_codes` | 対応 |

## 要件 3: ソースの読み込みと解析

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 3.1 読めるソースを解析して実行 | `src/source.rs::read_source_file_returns_bytes_unchanged`、`tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 3.2 UTF-8 不正バイトはコメント扱い | `src/source.rs::every_single_byte_value_only_fails_for_bracket_errors`、`src/source.rs::all_256_byte_values_parse_when_brackets_are_balanced`、`tests/source.rs::closing_bracket_after_invalid_utf8_comment_reports_real_byte_offset` | 対応 |
| 3.3 診断のバイトオフセットが実位置と一致 | `src/source.rs::invalid_utf8_byte_before_bracket_error_reports_real_offset`、`src/source.rs::japanese_utf8_before_bracket_error_reports_real_byte_offset`、`src/source.rs::unmatched_opening_bracket_after_non_utf8_bytes_reports_real_offset`、`tests/source.rs::opening_bracket_after_non_utf8_and_japanese_comments_reports_real_byte_offset` | 対応 |
| 3.4 空・コメントのみは 0、標準出力空 | `src/source.rs::empty_source_parses_to_zero_instructions`、`src/source.rs::comment_only_source_parses_to_zero_instructions`、`tests/source.rs::empty_source_file_exits_zero_with_no_output`、`tests/source.rs::comment_only_source_file_exits_zero_with_no_output`、`tests/regression.rs::empty_program_file_exits_zero_with_no_output` | 対応 |
| 3.5 括弧不一致は 2、命令未実行、標準出力空、種類・添字・オフセットを表示 | `tests/exit_codes.rs::unmatched_closing_bracket_exits_2_with_index_and_offset`、`tests/exit_codes.rs::unmatched_opening_bracket_exits_2_with_empty_stdout`、`src/error.rs::parse_diagnostic_embeds_core_message_with_numbers`、`tests/streams.rs::bracket_error_writes_no_program_output_and_only_stderr_diagnostic` | 対応 |
| 3.6 読込失敗は 4、対象と原因を表示 | `src/source.rs::read_missing_source_file_is_program_file_io_error_without_path`、`src/source.rs::read_directory_is_program_file_io_error`、`tests/source.rs::missing_source_file_exits_four_and_names_program_file_without_path`、`tests/source.rs::directory_as_source_exits_four_and_names_program_file_without_path`、`tests/exit_codes.rs::directory_as_source_exits_4_with_empty_stdout` | 対応 |
| 3.7 段の順序(引数 → 読込 → 解析 → 入力 → 実行 → 出力)と最初の失敗の分類 | `tests/source.rs::bracket_error_takes_precedence_over_missing_input_file`、`src/args.rs::first_error_is_kept_and_later_errors_do_not_replace_it`、`tests/input.rs::missing_input_file_is_io_error_for_program_without_input_command`(入力取得の失敗は、出力を生むプログラムでも実行前に終了 4 となり標準出力が空)、`src/app.rs::runtime_error_writes_partial_output_before_diagnostic`(実行段の部分出力が診断より先に出力段で書かれる) | 部分(解析と入力取得の順序、入力取得と実行の順序、実行結果と出力の順序は個別に検証。段を跨いだ順序を一括で確かめる結合テストは無い) |

## 要件 4: 入力の供給

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 4.1 `--input` の全バイトを無変換で供給 | `src/input.rs::input_file_bytes_are_returned_verbatim_including_nul_and_ff`、`tests/input.rs::echo_from_input_file_matches_stdin_source`、`tests/input.rs::nul_byte_from_input_is_passed_through_unchanged` | 対応 |
| 4.2 `,` を含み `--input` なしなら標準入力を EOF まで読む | `src/input.rs::stdin_is_read_to_eof_verbatim_when_program_has_input_instruction`、`tests/input.rs::echo_from_stdin_stops_before_first_nul_byte` | 対応 |
| 4.3 `--input` 指定時は標準入力を読まない | `src/input.rs::input_file_is_read_without_touching_stdin_when_program_needs_input`、`tests/input.rs::input_file_is_read_while_stdin_stays_open_and_unused` | 対応 |
| 4.4 `,` を含まないなら標準入力を読まず待機しない | `src/input.rs::stdin_is_not_read_when_program_has_no_input_instruction`、`tests/input.rs::program_without_input_command_finishes_with_open_unused_stdin`、`tests/examples.rs::hello_example_finishes_with_open_unused_stdin` | 対応 |
| 4.5 入力尽きた後の `,` は 0(EOF=0) | `tests/input.rs::read_after_input_is_exhausted_yields_zero` | 対応 |
| 4.6 空入力では `,` が EOF として実行される | `src/input.rs::empty_input_file_yields_empty_bytes`、`src/input.rs::empty_stdin_yields_empty_bytes_when_program_has_input_instruction`、`tests/input.rs::empty_stdin_makes_input_command_read_eof_as_zero`、`tests/input.rs::empty_input_file_makes_input_command_read_eof_as_zero` | 対応 |
| 4.7 `--input` の読込失敗は 4(`,` の有無によらない) | `src/input.rs::missing_input_file_is_input_io_error_with_input_instruction`、`src/input.rs::missing_input_file_is_input_io_error_without_input_instruction`、`tests/input.rs::missing_input_file_is_io_error_for_program_with_input_command`、`tests/input.rs::missing_input_file_is_io_error_for_program_without_input_command`、`tests/exit_codes.rs::missing_input_absolute_path_exits_4_without_leaking_path` | 対応 |
| 4.8 標準入力の読取失敗は 4 | `src/input.rs::stdin_read_failure_is_stdin_io_error_when_program_has_input_instruction`、`src/input.rs::stdin_failure_is_not_reported_when_program_has_no_input_instruction`、`src/input.rs::input_file_takes_precedence_over_stdin_failure` | 部分(結合テストなし。実プロセスの標準入力読取失敗は単体テストでのみ検証) |

## 要件 5: 出力と診断のストリーム分離

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 5.1 出力バイトを順序どおり標準出力へ書く | `tests/streams.rs::all_256_byte_values_reach_stdout_unchanged`、`src/app.rs::success_writes_output_and_leaves_stderr_empty` | 対応 |
| 5.2 UTF-8 検証・文字コード変換・改行変換・付加をしない | `tests/streams.rs::crlf_bytes_are_not_converted_or_appended` | 対応 |
| 5.3 0x00 / 0xFF を欠落・置換なしに書く | `tests/streams.rs::all_256_byte_values_reach_stdout_unchanged`、`tests/input.rs::nul_byte_from_input_is_passed_through_unchanged` | 対応 |
| 5.4 診断・状態・使用方法は標準エラーのみ | `tests/streams.rs::argument_errors_write_only_diagnostics_to_stderr`、`tests/streams.rs::bracket_error_writes_no_program_output_and_only_stderr_diagnostic`、`tests/streams.rs::read_errors_write_only_diagnostics_to_stderr`、`tests/streams.rs::runtime_error_keeps_partial_output_on_stdout_and_diagnostic_on_stderr` | 対応 |
| 5.5 正常完了かつ `--dump-state` なしなら標準エラーは空 | `src/app.rs::success_writes_output_and_leaves_stderr_empty`、`tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 5.6 実行エラー時は部分出力を先に、診断を後に書く | `src/app.rs::runtime_error_writes_partial_output_before_diagnostic`、`src/app.rs::pointer_out_of_bounds_keeps_partial_output_and_snapshot`、`tests/streams.rs::runtime_error_keeps_partial_output_on_stdout_and_diagnostic_on_stderr` | 対応 |
| 5.7 出力が空なら標準出力へ 1 バイトも書かない | `src/app.rs::empty_output_never_touches_stdout`、`tests/streams.rs::empty_output_program_writes_no_bytes_to_stdout`、`tests/streams.rs::empty_source_file_writes_no_bytes_to_stdout` | 対応 |

## 要件 6: 命令数の上限

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 6.1 既定上限は 10,000,000 | `src/args.rs::default_max_steps_is_ten_million`、`tests/limits.rs::default_limit_stops_infinite_loop_with_exit_3_and_diagnostic`、`tests/limits.rs::default_limit_dump_state_reports_exact_limit_steps` | 対応 |
| 6.2 `--max-steps N` で上限 N | `src/args.rs::max_steps_resolution`、`tests/limits.rs::max_steps_below_program_length_stops_with_exit_3_and_diagnostic` | 対応 |
| 6.3 `--max-steps 0` かつ命令ありは未実行で 3 | `tests/limits.rs::max_steps_zero_with_nonempty_program_executes_nothing`、`src/app.rs::run_limit_zero_exceeds_limit_before_any_step` | 対応 |
| 6.4 ちょうど N 命令で停止すれば 0 | `tests/limits.rs::max_steps_exactly_matching_program_length_succeeds`、`tests/limits.rs::max_steps_zero_with_empty_program_succeeds`、`src/app.rs::run_within_limit_halts_successfully` | 対応 |
| 6.5 `--no-step-limit` で上限なし | `tests/limits.rs::no_step_limit_lets_finite_program_finish_normally`、`src/app.rs::unlimited_run_exceeds_default_limit_and_completes_with_exact_steps`(既定上限 10,000,000 を超える 33,554,432 命令を上限なしで完走し、実行数・pc・dp を検証。差し戻し前の `unlimited_run_completes_past_default_style_limits` は 3 命令だけで上限超えを検証しておらず、本行の根拠として不十分だった) | 対応 |
| 6.6 上限到達で追加実行せず、上限値と `pc` を含む診断、終了コード 3 | `src/error.rs::step_limit_diagnostic_embeds_limit_and_pc`、`src/app.rs::run_limit_two_exceeds_limit_with_runtime_error`、`tests/limits.rs::max_steps_below_program_length_stops_with_exit_3_and_diagnostic`、`tests/limits.rs::dump_state_on_step_limit_prints_interrupted_values` | 対応 |
| 6.7 `--max-steps` と `--no-step-limit` の併用は 2 | `src/args.rs::max_steps_and_no_step_limit_conflict`、`tests/args.rs::max_steps_and_no_step_limit_together_is_usage_error`、`tests/limits.rs::max_steps_with_no_step_limit_is_usage_error_in_either_order` | 対応 |
| 6.8 `--max-steps` の値が不正なら 2、値を表示しない | `src/args.rs::invalid_max_steps_values`、`tests/limits.rs::invalid_max_steps_values_are_usage_errors_without_echoing_the_value`、`tests/exit_codes.rs::invalid_max_steps_exits_2_without_echoing_value_or_cwd` | 対応 |

## 要件 7: 実行状態の表示(`--dump-state`)

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 7.1 停止時に `pc` / `dp` / `steps` を名前付きで標準エラーへ | `src/app.rs::dump_state_writes_snapshot_to_stderr_on_success`、`tests/limits.rs::dump_state_on_normal_stop_prints_pc_dp_steps_to_stderr`、`tests/examples.rs::dump_state_for_hello_matches_regression_oracle` | 対応 |
| 7.2 実行エラー・上限超過で中断時点の値を表示 | `tests/limits.rs::dump_state_on_step_limit_prints_interrupted_values`、`tests/limits.rs::dump_state_on_runtime_error_prints_interrupted_values`、`src/app.rs::runtime_error_with_dump_state_prints_diagnostic_then_state`、`src/app.rs::step_limit_exceeded_dump_state_shows_partial_progress` | 対応 |
| 7.3 実行に至らない失敗では状態表示をしない | `tests/limits.rs::dump_state_is_not_printed_on_argument_error`、`tests/limits.rs::dump_state_is_not_printed_on_parse_error`、`tests/limits.rs::dump_state_is_not_printed_on_source_read_error`、`tests/limits.rs::dump_state_is_not_printed_when_input_file_missing_for_program_with_input_command`、`tests/limits.rs::dump_state_is_not_printed_when_input_file_missing_for_program_without_input_command` | 対応(入力取得の失敗も、入力命令の有無によらず `--dump-state` 併用で状態表示が出ないことを検証) |
| 7.4 `--dump-state` 未指定なら表示しない | `tests/limits.rs::dump_state_is_not_printed_when_not_requested`、`src/app.rs::success_writes_output_and_leaves_stderr_empty` | 対応 |
| 7.5 表示値を実行エンジンの値と一致させる | `tests/examples.rs::dump_state_for_hello_matches_regression_oracle`、`tests/examples.rs::cli_agrees_with_core_on_step_limit_exceeded`、`tests/examples.rs::cli_agrees_with_core_on_pointer_out_of_bounds_after_partial_output` | 対応 |
| 7.6 標準出力の書込失敗時も(実行が停止・中断していれば)状態表示を書く | `src/app.rs::stdout_write_failure_without_runtime_error_is_exit_four`、`src/app.rs::stdout_write_failure_after_runtime_error_reports_both_diagnostics` | 対応 |
| 7.7 書式を固定し、同一入力で同一表示 | `src/app.rs::format_state_uses_snapshot_values`、`src/app.rs::format_state_reports_data_pointer_and_partial_progress`、`src/app.rs::repeated_runs_are_deterministic` | 対応 |

## 要件 8: 終了コード

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 8.1 停止かつ出力を全て書けたら 0 | `tests/exit_codes.rs::hello_example_exits_0_with_program_output_only`、`tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 8.2 引数エラー・括弧不一致は 2 | `tests/exit_codes.rs::argument_errors_exit_2_with_empty_stdout`、`tests/exit_codes.rs::unmatched_closing_bracket_exits_2_with_index_and_offset`、`src/error.rs::every_args_error_maps_to_usage_exit_code`、`src/error.rs::parse_error_maps_to_usage_exit_code` | 対応 |
| 8.3 範囲外・上限超過の実行エラーは 3 | `tests/exit_codes.rs::pointer_out_of_bounds_exits_3_with_partial_output_and_state_in_diagnostic`、`tests/limits.rs::default_limit_stops_infinite_loop_with_exit_3_and_diagnostic`、`src/error.rs::runtime_and_incomplete_map_to_runtime_exit_code` | 対応 |
| 8.4 読取・書込の失敗は 4 | `tests/exit_codes.rs::missing_source_absolute_path_exits_4_without_leaking_path_or_cwd`、`tests/exit_codes.rs::closed_stdout_exits_4_and_diagnostic_targets_standard_output`、`tests/input.rs::missing_input_file_is_io_error_for_program_with_input_command`、`src/error.rs::io_error_maps_to_io_exit_code_for_every_target` | 対応 |
| 8.5 実行エラーの後に標準出力書込も失敗したら両方の診断を書き 4 | `src/app.rs::stdout_write_failure_after_runtime_error_reports_both_diagnostics`、`src/app.rs::stdout_write_failure_without_runtime_error_is_exit_four` | 対応 |
| 8.6 予約済みの未完了停止理由は 3、未完了の診断 | `src/app.rs::classify_reserved_stop_reasons_are_incomplete`、`src/error.rs::incomplete_diagnostic_states_early_stop`、`src/error.rs::runtime_and_incomplete_map_to_runtime_exit_code` | 部分(単体テストのみ。core が現在この停止理由を返さないため、結合テストでは到達できない) |
| 8.7 終了コードは 0 / 2 / 3 / 4 のみ | `src/error.rs::exit_codes_are_only_zero_two_three_or_four`、`tests/exit_codes.rs::exit_codes_observed_are_only_0_2_3_or_4` | 対応 |
| 8.8 同一条件で毎回同じ出力・終了コード | `src/app.rs::repeated_runs_are_deterministic`、`tests/exit_codes.rs::repeated_runs_produce_identical_stdout_stderr_and_code`、`src/input.rs::acquire_input_is_deterministic_for_same_stdin_bytes` | 対応 |

## 要件 9: 診断の内容と機微情報の除外

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 9.1 実行エラーの診断に原因種別、`pc` / `dp` / 方向、または上限値と `pc` を含む | `src/error.rs::runtime_diagnostic_embeds_core_message_with_numbers`、`src/error.rs::step_limit_diagnostic_embeds_limit_and_pc`、`tests/exit_codes.rs::pointer_out_of_bounds_exits_3_with_partial_output_and_state_in_diagnostic`(`direction=left` を確認) | 対応 |
| 9.2 括弧エラーの診断に種類・命令添字・ソースバイトオフセット | `src/error.rs::parse_diagnostic_embeds_core_message_with_numbers`、`tests/exit_codes.rs::unmatched_closing_bracket_exits_2_with_index_and_offset` | 対応 |
| 9.3 入出力エラーの診断に対象の種別と OS の原因 | `src/error.rs::io_diagnostics_name_target_role_and_os_cause`、`src/error.rs::io_error_keeps_os_error_as_is`、`tests/exit_codes.rs::closed_stdout_exits_4_and_diagnostic_targets_standard_output` | 対応 |
| 9.4 診断にパス文字列・絶対パス・親ディレクトリ名・カレントディレクトリを含めない | `tests/exit_codes.rs::missing_source_absolute_path_exits_4_without_leaking_path_or_cwd`、`tests/exit_codes.rs::missing_input_absolute_path_exits_4_without_leaking_path`、`tests/exit_codes.rs::invalid_max_steps_exits_2_without_echoing_value_or_cwd`、`tests/source.rs::missing_source_file_exits_four_and_names_program_file_without_path`、`src/source.rs::read_missing_source_file_is_program_file_io_error_without_path` | 対応 |
| 9.5 診断にソース全文・抜粋・入力・出力バイト列を含めない | `tests/exit_codes.rs::diagnostics_exclude_source_text_and_io_bytes` | 対応 |
| 9.6 引数不正の診断に指定値を含めず、種類を示す | `src/args.rs::errors_never_contain_supplied_values`、`src/error.rs::args_diagnostics_never_contain_supplied_values`、`tests/args.rs::argument_diagnostics_do_not_echo_supplied_values`、`src/app.rs::unknown_option_diagnostic_does_not_echo_the_value` | 対応 |
| 9.7 診断文面を固定 | `src/error.rs::args_diagnostics_use_fixed_templates` | 対応 |

## 要件 10: サンプルプログラムと README

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 10.1 `examples/hello.bf` が `Hello World!\n` のみを出力するプログラムである | `tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 10.2 既定オプションで stdout のみ、stderr 空、終了コード 0 | `tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr`、`tests/exit_codes.rs::hello_example_exits_0_with_program_output_only` | 対応 |
| 10.3 標準入力が端末でも入力待ちせず終了する | `tests/examples.rs::hello_example_finishes_with_open_unused_stdin` | 部分(書き込みなしで開いたままのパイプで検証。実際の端末での確認は自動テストにない) |
| 10.4 README に 2 つの検証コマンドを記載 | `tests/examples.rs::readme_contains_both_verification_commands` | 対応 |
| 10.5 README に使用方法・全オプション・既定上限・終了コード・標準出力と標準エラーの役割 | `tests/examples.rs::readme_option_table_lists_every_option`、`tests/examples.rs::readme_exit_code_table_lists_0_2_3_and_4` | 部分(オプション表と終了コード表のみ自動検証。既定上限と出力の役割分担の記述は README 本文を目視で確認) |
| 10.6 端末表示の文字コード注意とリダイレクトの推奨を記載 | 自動テストなし | 部分(README「出力の扱いに関する注意」節に記載があることを目視で確認。自動テストは無い) |

## 要件 11: 実行エンジンの結果との一致と依存の制約

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 11.1 実行意味論を独自実装せず core の結果を用いる | 自動テストなし。`crates/brainfuck-cli/Cargo.toml` の依存は `brainfuck-core`(path)のみ、実行は `src/app.rs::run_program` を経由 | 部分(静的確認。設計・レビューで担保、自動テストなし) |
| 11.2 core 直接実行と CLI の出力・実行数・エラー種別が一致 | `tests/examples.rs::cli_agrees_with_core_on_hello`、`tests/examples.rs::cli_agrees_with_core_on_echo_with_stdin_input`、`tests/examples.rs::cli_agrees_with_core_on_echo_stopping_at_end_of_input`、`tests/examples.rs::cli_agrees_with_core_on_pointer_out_of_bounds_after_partial_output`、`tests/examples.rs::cli_agrees_with_core_on_step_limit_exceeded`、`tests/examples.rs::cli_agrees_with_core_on_step_limit_boundary`、`tests/examples.rs::cli_agrees_with_core_on_unmatched_closing_bracket`、`tests/examples.rs::cli_agrees_with_core_on_unmatched_opening_bracket`、`tests/examples.rs::cli_agrees_with_core_on_empty_program` | 対応 |
| 11.3 `unsafe` を含まない | `crates/brainfuck-cli/src/main.rs` の `#![forbid(unsafe_code)]`、ソース中に `unsafe` の出現なし(本タスクで grep 確認) | 対応(静的確認) |
| 11.4 ワークスペースルートで `cargo test --workspace` が通る | 本タスクで `cargo test --workspace` を実行し成功(全テスト 0 failed) | 対応 |

## 要件 12: テスト

| 受け入れ基準 | 対応テスト | 状態 |
| --- | --- | --- |
| 12.1 プロセス単位の結合テストで stdout / stderr / 終了コードを検証 | `tests/*.rs`(`tests/common/mod.rs::run_cli` が実バイナリを子プロセスとして起動) | 対応 |
| 12.2 `examples/hello.bf` の stdout が `Hello World!\n`、終了コード 0 | `tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 12.3 0x00 と 0xFF を含む任意のバイト列を標準入力・`--input` から与えるエコーが同一バイト列を出力 | `tests/input.rs::all_256_byte_values_echo_from_stdin_unchanged`、`tests/input.rs::all_256_byte_values_echo_from_input_file_unchanged`(`,.`×256 で 0x00〜0xFF の全 256 値を両経路で検証)、`tests/input.rs::echo_from_stdin_stops_before_first_nul_byte`、`tests/input.rs::echo_from_input_file_matches_stdin_source`(`fixtures/echo.bf` は 0x00 の手前まで)、`tests/input.rs::nul_byte_from_input_is_passed_through_unchanged` | 対応 |
| 12.4 終了コード 0 / 2 / 3 / 4 の代表ケース | `tests/exit_codes.rs::hello_example_exits_0_with_program_output_only`、`tests/exit_codes.rs::unmatched_closing_bracket_exits_2_with_index_and_offset`、`tests/exit_codes.rs::pointer_out_of_bounds_exits_3_with_partial_output_and_state_in_diagnostic`、`tests/exit_codes.rs::missing_source_absolute_path_exits_4_without_leaking_path_or_cwd` | 対応 |
| 12.5 成功・失敗それぞれで stdout / stderr の分離 | `tests/streams.rs::argument_errors_write_only_diagnostics_to_stderr`、`tests/streams.rs::runtime_error_keeps_partial_output_on_stdout_and_diagnostic_on_stderr`、`tests/streams.rs::all_256_byte_values_reach_stdout_unchanged`、`tests/examples.rs::hello_example_prints_exactly_hello_world_with_empty_stderr` | 対応 |
| 12.6 標準入力を読まない条件で、標準入力が閉じていなくても終了 | `tests/input.rs::program_without_input_command_finishes_with_open_unused_stdin`、`tests/input.rs::input_file_is_read_while_stdin_stays_open_and_unused`、`tests/input.rs::input_file_with_program_without_input_command_finishes_with_open_unused_stdin`、`tests/examples.rs::hello_example_finishes_with_open_unused_stdin` | 対応 |
| 12.7 診断にコマンドラインのパス文字列を含めない | `tests/exit_codes.rs::missing_source_absolute_path_exits_4_without_leaking_path_or_cwd`、`tests/source.rs::missing_source_file_exits_four_and_names_program_file_without_path` | 対応 |
| 12.8 CLI が終了しない場合は有限時間内に失敗 | `tests/common/mod.rs::wait_with_timeout`(30 秒の期限を超えたら子プロセスを kill してパニック) | 対応 |
| 12.9 外部ツール・ネットワーク・環境変数に依存せず全テスト完走 | 本タスクで `cargo test --workspace` を実行し成功。依存は path 依存のみ | 対応 |
| 12.10 空ソースの回帰テスト 1 件 | `tests/regression.rs::empty_program_file_exits_zero_with_no_output` | 対応 |

## 未対応・部分の一覧(差し戻し先)

タスク 7.1 の差し戻し(R1〜R4)を修復した後の状態。以下は対応するタスクへの差し戻し候補。

| 受け入れ基準 | 状態 | 差し戻し先 | 内容 |
| --- | --- | --- | --- |
| 3.7 | 部分 | 6.2 | 段を跨いだ順序(引数 → 読込 → 解析 → 入力 → 実行 → 出力)を一括で確かめる結合テストが無い。各段の順序は個別に検証済み |
| 4.8 | 部分 | 3.3 | 標準入力の読取失敗の結合テストが無い(単体テストのみ) |
| 8.6 | 部分 | 4.1 / 4.3 | 予約済みの未完了停止理由は単体テストのみ(core が現在返さないため到達不能) |
| 10.3 | 部分 | 6.7 | 実端末での入力待ち回避は自動検証していない(開いたパイプで代替) |
| 10.5 | 部分 | 5.2 | README の既定上限と出力役割分担の記述は自動検証していない(目視確認のみ) |
| 10.6 | 部分 | 5.2 | README の端末文字コード注意は自動検証していない(目視確認のみ) |
| 11.1 | 部分 | 4.1 | 実行意味論を独自実装していないことの自動テストは無い(静的確認のみ) |

件数: 部分 7 件(3.7、4.8、8.6、10.3、10.5、10.6、11.1)、未対応 0 件。差し戻し前は部分 9 件だった(12.3、7.3 が対応に変わり、3.7 の根拠を追記)。いずれも「未対応(テストが全く無い)」の基準は無い。

## 確認コマンドの結果(タスク 7.1)

- `cargo build --workspace`: 成功、警告 0 件
- `cargo test --workspace`: 成功、失敗 0 件、警告 0 件
- `cargo clippy --workspace --all-targets -- -D warnings`: 成功
- `cargo fmt -p brainfuck-cli --check`: 成功(差分なし)
- `cargo run -p brainfuck-cli -- examples/hello.bf`(標準入力 /dev/null): 標準出力 `Hello World!\n`(13 バイト)、標準エラー 0 バイト、終了コード 0
- 差し戻し修復後(R1〜R4)の再実行: `cargo build --workspace` 成功、`cargo clippy --workspace --all-targets -- -D warnings` 成功、`cargo test --workspace` 成功(失敗 0 件)、`cargo fmt -p brainfuck-cli --check` 成功
- 変異テスト(一時的に変異を入れて失敗を確認し、元に戻した): `src/app.rs` の `None => MachineConfig::unlimited()` を `with_max_steps(10_000_000)` に置換 → `unlimited_run_exceeds_default_limit_and_completes_with_exact_steps` が `StepLimitExceeded { limit: 10000000 }` で失敗。入力取得の失敗時に `state:` を出す変異 → `tests/limits.rs` の入力ファイル失敗 2 件が失敗
