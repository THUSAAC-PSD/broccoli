# IOI Plugin

An IOI (International Olympiad in Informatics) scoring plugin for Broccoli online judge system.

## Features

- **Subtask-based Scoring**: Support for organizing test cases into subtasks with independent scoring
- **Multiple Scoring Methods**: 
  - `Sum`: Accumulate scores from all test cases (partial credit)
  - `GroupMin`: All-or-nothing scoring (full points if all pass, 0 otherwise)
  - `GroupMul`: Product-based partial credit scoring
- **Final Score Calculation Methods**:
  - `BestSubmission`: Use the highest total score from any single submission
  - `BestSubtaskSum`: Use the best score for each subtask across all submissions (IOI 2010-2016 style)
- **Per-Problem Configuration**: Each problem can have its own subtask and scoring configuration

## API Functions

### `get_leaderboard`

Get the contest leaderboard with IOI scoring.

**Input:**
```json
{
  "contest_id": 1,
  "page": 1,
  "page_size": 50
}
```

**Output:**
```json
{
  "contest_id": 1,
  "problems": [...],
  "entries": [
    {
      "rank": 1,
      "user": {"id": 1, "username": "alice"},
      "problem_scores": [
        {"problem_id": 1, "score": 100, "max_score": 100, ...}
      ],
      "total_score": 100
    }
  ],
  "total_count": 10,
  "page": 1,
  "page_size": 50
}
```

### `get_submission_detail`

Get detailed information about a submission, including subtask results.

**Input:**
```json
{
  "submission_id": 1,
  "include_code": false
}
```

**Output:**
```json
{
  "submission": {...},
  "judge_result": {...},
  "test_case_results": [...],
  "subtask_results": [
    {
      "subtask_id": 1,
      "subtask_name": "Subtask 1 - Small",
      "score": 30,
      "max_score": 30,
      "verdict": "Accepted",
      "time_used": 100,
      "memory_used": 1024
    }
  ],
  "problem_config": {...}
}
```

### `configure_problem`

Configure IOI scoring settings for a problem.

**Input:**
```json
{
  "problem_id": 1,
  "subtask_enabled": true,
  "final_score_method": "BestSubmission",
  "subtasks": [
    {
      "id": 1,
      "name": "Subtask 1 - Small (N ≤ 100)",
      "max_score": 30,
      "scoring_method": "Sum",
      "test_case_ids": [1, 2, 3]
    },
    {
      "id": 2,
      "name": "Subtask 2 - Full",
      "max_score": 70,
      "scoring_method": "GroupMin",
      "test_case_ids": [4, 5, 6, 7]
    }
  ]
}
```

### `get_problem_config_api`

Get the current IOI configuration for a problem.

**Input:**
```json
{
  "problem_id": 1
}
```

### `calculate_submission_score`

Calculate and persist the IOI score for a submission. This function should be called after judging completes (typically via a hook).

**Input:**
```json
{
  "submission_id": 1
}
```

**Output:**
```json
{
  "success": true,
  "submission_id": 1,
  "score": 60,
  "verdict": "PartiallyCorrect",
  "subtask_results": [...],
  "message": "Score calculated and saved"
}
```

## Scoring Methods

### Sum

Each test case contributes its individual score to the subtask total.

```
subtask_score = sum(test_case_scores)
```

Best for problems where partial progress within a subtask should be rewarded.

### GroupMin (All-or-Nothing)

The subtask receives full points only if ALL test cases pass (verdict = "Accepted").

```
subtask_score = max_score if all_accepted else 0
```

This is the most common method in IOI-style contests. Used when a subtask represents a specific complexity bound (e.g., N ≤ 1000).

### GroupMul (Product)

Score is calculated as the product of individual test case score ratios.

```
subtask_score = max_score × ∏(test_case_score / expected_score)
```

This method heavily penalizes any failing test case while still allowing partial credit.

## Final Score Methods

### BestSubmission

The contestant's final score for a problem is the highest total score achieved in any single submission.

```
final_score = max(submission_total_scores)
```

### BestSubtaskSum

For each subtask, take the best score across all submissions, then sum them up. This was used in IOI from 2010-2016.

```
final_score = sum(max(subtask_score across submissions) for each subtask)
```

This allows contestants to combine the best subtask results from different submissions.

## Building

```bash
# Build for WASM
cargo build --target wasm32-wasip1 --release

# The plugin will be at:
# target/wasm32-wasip1/release/ioi_plugin.wasm
```

## Testing

```bash
# Run the test suite (requires server to be running)
python tests/test_api.py --base-url http://localhost:3000

# Run in quiet mode
python tests/test_api.py -q
```

## Project Structure

```
ioi-plugin/
├── Cargo.toml          # Rust project configuration
├── README.md           # This file
├── src/
│   ├── lib.rs          # Main plugin logic and API functions
│   ├── models.rs       # Data structures and types
│   └── mock.rs         # Mock data for testing
└── tests/
    └── test_api.py     # API test suite
```

## Integration

The plugin integrates with the Broccoli server through host functions:

- `log_info`: Log messages
- `store_get/store_set`: Key-value storage for configurations
- `db_query`: Query database tables
- `db_update`: Update database records

### Workflow

1. **Problem Setup**: Contest admin configures each problem using `configure_problem`
2. **Submission Judging**: The judge evaluates the submission and stores test case results
3. **Score Calculation**: After judging, `calculate_submission_score` is called to:
   - Compute subtask scores based on scoring methods
   - Calculate total score
   - Write results back to `judge_result`
4. **Leaderboard**: `get_leaderboard` reads stored scores and generates rankings

## License

MIT
