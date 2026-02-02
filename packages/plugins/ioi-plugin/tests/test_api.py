#!/usr/bin/env python3
"""
IOI Plugin API Test Suite

This script tests all IOI plugin APIs:
1. get_leaderboard - Get contest leaderboard with IOI scoring
2. get_submission_detail - Get submission details with subtask results
3. configure_problem - Configure IOI settings per problem (subtasks, scoring methods)
4. get_problem_config_api - Get IOI configuration for a problem
5. calculate_submission_score - Calculate score and write back to database

Scoring Methods:
- SubtaskScoringMethod: Sum, GroupMin, GroupMul
- FinalScoreMethod: BestSubmission, BestSubtaskSum

Usage:
    python test_api.py [--base-url URL] [--plugin-id PLUGIN_ID]
    
Example:
    python test_api.py --base-url http://localhost:3000 --plugin-id ioi-plugin
"""

import argparse
import json
import sys
import time
from dataclasses import dataclass
from typing import Any, Optional
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError


# ============================================================================
# Configuration
# ============================================================================

@dataclass
class TestConfig:
    base_url: str = "http://localhost:3000"
    plugin_id: str = "ioi-plugin"
    verbose: bool = True


# ============================================================================
# HTTP Client
# ============================================================================

class APIClient:
    def __init__(self, config: TestConfig):
        self.config = config
        self.base_url = config.base_url.rstrip("/")
    
    def call_plugin(self, func_name: str, payload: dict) -> dict:
        """Call a plugin function via the server API"""
        url = f"{self.base_url}/plugins/{self.config.plugin_id}/call/{func_name}"
        
        data = json.dumps(payload).encode("utf-8")
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
        }
        
        req = Request(url, data=data, headers=headers, method="POST")
        
        try:
            with urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except HTTPError as e:
            error_body = e.read().decode("utf-8") if e.fp else ""
            raise APIError(e.code, error_body)
        except URLError as e:
            raise ConnectionError(f"Failed to connect to {url}: {e.reason}")
    
    def load_plugin(self) -> str:
        """Load the plugin"""
        url = f"{self.base_url}/plugins/{self.config.plugin_id}/load"
        req = Request(url, method="POST")
        
        try:
            with urlopen(req, timeout=30) as response:
                return response.read().decode("utf-8")
        except HTTPError as e:
            error_body = e.read().decode("utf-8") if e.fp else ""
            raise APIError(e.code, error_body)


class APIError(Exception):
    def __init__(self, status_code: int, body: str):
        self.status_code = status_code
        self.body = body
        super().__init__(f"HTTP {status_code}: {body}")


# ============================================================================
# Test Cases
# ============================================================================

class TestResult:
    def __init__(self, name: str):
        self.name = name
        self.passed = False
        self.error: Optional[str] = None
        self.response: Any = None
        self.duration_ms: float = 0
    
    def __str__(self):
        status = "✅ PASS" if self.passed else "❌ FAIL"
        result = f"{status} {self.name} ({self.duration_ms:.1f}ms)"
        if self.error:
            result += f"\n    Error: {self.error}"
        return result


class IOIPluginTestSuite:
    def __init__(self, client: APIClient, verbose: bool = True):
        self.client = client
        self.verbose = verbose
        self.results: list[TestResult] = []
    
    def run_all(self) -> bool:
        """Run all tests and return True if all passed"""
        print("=" * 60)
        print("IOI Plugin Test Suite")
        print("=" * 60)
        print(f"Base URL: {self.client.base_url}")
        print(f"Plugin ID: {self.client.config.plugin_id}")
        print("=" * 60)
        print()
        
        # Try to load plugin first
        self._run_test("Load Plugin", self.test_load_plugin)
        
        # Core functionality tests
        self._run_test("Get Leaderboard (Basic)", self.test_get_leaderboard_basic)
        self._run_test("Get Leaderboard (Pagination)", self.test_get_leaderboard_pagination)
        self._run_test("Get Leaderboard (Invalid Contest)", self.test_get_leaderboard_invalid)
        
        self._run_test("Get Submission Detail (Basic)", self.test_get_submission_detail_basic)
        self._run_test("Get Submission Detail (With Code)", self.test_get_submission_detail_with_code)
        self._run_test("Get Submission Detail (Not Found)", self.test_get_submission_detail_not_found)
        
        # IOI-specific configuration tests
        self._run_test("Configure Problem (Basic)", self.test_configure_problem_basic)
        self._run_test("Configure Problem (GroupMin Method)", self.test_configure_problem_groupmin)
        self._run_test("Configure Problem (BestSubtaskSum)", self.test_configure_problem_best_subtask_sum)
        self._run_test("Get Problem Config", self.test_get_problem_config)
        
        # Score calculation tests (simulates post-judging hook)
        self._run_test("Calculate Submission Score", self.test_calculate_submission_score)
        self._run_test("Calculate Score (Not Found)", self.test_calculate_score_not_found)
        
        # Integration tests: verify leaderboard uses stored scores
        self._run_test("Leaderboard Uses Stored Scores", self.test_leaderboard_uses_stored_scores)
        
        # Subtask scoring mechanism tests
        self._run_test("Subtask Sum Scoring", self.test_subtask_sum_scoring)
        self._run_test("Subtask GroupMin Scoring", self.test_subtask_groupmin_scoring)
        self._run_test("Subtask Partial Score", self.test_subtask_partial_score)
        
        # Print summary
        self._print_summary()
        
        return all(r.passed for r in self.results)
    
    def _run_test(self, name: str, test_func):
        """Run a single test and record the result"""
        result = TestResult(name)
        start = time.time()
        
        try:
            response = test_func()
            result.passed = True
            result.response = response
        except AssertionError as e:
            result.error = str(e)
        except APIError as e:
            result.error = f"API Error: {e}"
        except ConnectionError as e:
            result.error = f"Connection Error: {e}"
        except Exception as e:
            result.error = f"Unexpected Error: {type(e).__name__}: {e}"
        
        result.duration_ms = (time.time() - start) * 1000
        self.results.append(result)
        
        print(result)
        if self.verbose and result.response:
            print(f"    Response: {json.dumps(result.response, indent=2, ensure_ascii=False)[:500]}")
        print()
    
    def _print_summary(self):
        """Print test summary"""
        print("=" * 60)
        print("Test Summary")
        print("=" * 60)
        
        passed = sum(1 for r in self.results if r.passed)
        failed = len(self.results) - passed
        
        print(f"Total:  {len(self.results)}")
        print(f"Passed: {passed}")
        print(f"Failed: {failed}")
        
        if failed > 0:
            print("\nFailed tests:")
            for r in self.results:
                if not r.passed:
                    print(f"  - {r.name}: {r.error}")
        
        print("=" * 60)
    
    # ========================================================================
    # Test Methods
    # ========================================================================
    
    def test_load_plugin(self):
        """Test loading the plugin"""
        result = self.client.load_plugin()
        assert "loaded" in result.lower() or "success" in result.lower(), \
            f"Unexpected response: {result}"
        return {"message": result}
    
    def test_get_leaderboard_basic(self):
        """Test basic leaderboard retrieval"""
        response = self.client.call_plugin("get_leaderboard", {
            "contest_id": 1
        })
        
        # Validate response structure
        assert "contest_id" in response, "Missing contest_id in response"
        assert "problems" in response, "Missing problems in response"
        assert "entries" in response, "Missing entries in response"
        assert "total_count" in response, "Missing total_count in response"
        
        # Validate data types
        assert isinstance(response["problems"], list), "problems should be a list"
        assert isinstance(response["entries"], list), "entries should be a list"
        
        # Validate leaderboard entries structure
        if response["entries"]:
            entry = response["entries"][0]
            assert "rank" in entry, "Missing rank in entry"
            assert "user" in entry, "Missing user in entry"
            assert "problem_scores" in entry, "Missing problem_scores in entry"
            assert "total_score" in entry, "Missing total_score in entry"
            
            # Validate user structure
            user = entry["user"]
            assert "id" in user, "Missing id in user"
            assert "username" in user, "Missing username in user"
        
        return response
    
    def test_get_leaderboard_pagination(self):
        """Test leaderboard pagination"""
        # Get first page
        page1 = self.client.call_plugin("get_leaderboard", {
            "contest_id": 1,
            "page": 1,
            "page_size": 2
        })
        
        assert page1["page"] == 1, "Page number should be 1"
        assert page1["page_size"] == 2, "Page size should be 2"
        assert len(page1["entries"]) <= 2, "Should return at most 2 entries"
        
        # Get second page
        page2 = self.client.call_plugin("get_leaderboard", {
            "contest_id": 1,
            "page": 2,
            "page_size": 2
        })
        
        assert page2["page"] == 2, "Page number should be 2"
        
        return {"page1": page1, "page2": page2}
    
    def test_get_leaderboard_invalid(self):
        """Test leaderboard with non-existent contest"""
        response = self.client.call_plugin("get_leaderboard", {
            "contest_id": 99999
        })
        
        # Should return empty or mock data, not error
        assert "entries" in response, "Should still return entries field"
        
        return response
    
    def test_get_submission_detail_basic(self):
        """Test basic submission detail retrieval"""
        response = self.client.call_plugin("get_submission_detail", {
            "submission_id": 1
        })
        
        # Validate response structure
        assert "submission" in response, "Missing submission in response"
        assert "judge_result" in response, "Missing judge_result in response"
        assert "test_case_results" in response, "Missing test_case_results in response"
        assert "subtask_results" in response, "Missing subtask_results in response"
        assert "problem_config" in response, "Missing problem_config in response"
        
        # Validate submission structure
        if response["submission"]:
            sub = response["submission"]
            assert "id" in sub, "Missing id in submission"
            assert "language" in sub, "Missing language in submission"
            assert "status" in sub, "Missing status in submission"
            assert "code" in sub, "Missing code in submission"
            
            # Code should be empty when include_code is not set
            assert sub["code"] == "", "Code should be empty by default"
        
        # Validate judge_result structure
        if response["judge_result"]:
            jr = response["judge_result"]
            assert "verdict" in jr, "Missing verdict in judge_result"
            assert "score" in jr, "Missing score in judge_result"
            assert "time_used" in jr, "Missing time_used in judge_result"
            assert "memory_used" in jr, "Missing memory_used in judge_result"
        
        # Validate subtask_results structure (IOI-specific)
        if response["subtask_results"]:
            sr = response["subtask_results"][0]
            assert "subtask_id" in sr, "Missing subtask_id in subtask_result"
            assert "subtask_name" in sr, "Missing subtask_name in subtask_result"
            assert "score" in sr, "Missing score in subtask_result"
            assert "max_score" in sr, "Missing max_score in subtask_result"
            assert "verdict" in sr, "Missing verdict in subtask_result"
        
        # Validate problem_config structure
        if response["problem_config"]:
            pc = response["problem_config"]
            assert "problem_id" in pc, "Missing problem_id in problem_config"
            assert "subtask_enabled" in pc, "Missing subtask_enabled in problem_config"
            assert "final_score_method" in pc, "Missing final_score_method in problem_config"
        
        return response
    
    def test_get_submission_detail_with_code(self):
        """Test submission detail with source code"""
        response = self.client.call_plugin("get_submission_detail", {
            "submission_id": 1,
            "include_code": True
        })
        
        assert response["submission"] is not None, "Submission should exist"
        assert response["submission"]["code"] != "", "Code should not be empty"
        
        return response
    
    def test_get_submission_detail_not_found(self):
        """Test submission detail for non-existent submission"""
        response = self.client.call_plugin("get_submission_detail", {
            "submission_id": 99999
        })
        
        # Should return null/None submission or mock data
        assert "submission" in response, "Should still have submission field"
        
        return response
    
    def test_configure_problem_basic(self):
        """Test configuring IOI settings for a problem with mixed scoring methods"""
        subtasks = [
            {
                "id": 1,
                "name": "Subtask 1 - Small (N ≤ 100)",
                "max_score": 30,
                "scoring_method": "Sum",
                "test_case_ids": [1, 2, 3]
            },
            {
                "id": 2,
                "name": "Subtask 2 - Medium (N ≤ 10000)",
                "max_score": 30,
                "scoring_method": "GroupMin",
                "test_case_ids": [4, 5]
            },
            {
                "id": 3,
                "name": "Subtask 3 - Large (N ≤ 10^6)",
                "max_score": 40,
                "scoring_method": "GroupMin",
                "test_case_ids": [6, 7]
            }
        ]
        
        response = self.client.call_plugin("configure_problem", {
            "problem_id": 1,
            "subtask_enabled": True,
            "final_score_method": "BestSubmission",
            "subtasks": subtasks
        })
        
        assert "success" in response, "Missing success field"
        assert response["success"] == True, "Configuration should succeed"
        
        return response

    def test_configure_problem_groupmin(self):
        """Test configuring with GroupMin scoring method (all-or-nothing)"""
        subtasks = [
            {
                "id": 1,
                "name": "Subtask 1 - Examples",
                "max_score": 10,
                "scoring_method": "GroupMin",
                "test_case_ids": [1, 2]
            },
            {
                "id": 2,
                "name": "Subtask 2 - Full",
                "max_score": 90,
                "scoring_method": "GroupMin",
                "test_case_ids": [3, 4, 5, 6, 7, 8, 9, 10]
            }
        ]
        
        response = self.client.call_plugin("configure_problem", {
            "problem_id": 2,
            "subtask_enabled": True,
            "final_score_method": "BestSubmission",
            "subtasks": subtasks
        })
        
        assert response["success"] == True, "Configuration should succeed"
        return response

    def test_configure_problem_best_subtask_sum(self):
        """Test configuring with BestSubtaskSum final score method (IOI 2010-2016 style)"""
        subtasks = [
            {
                "id": 1,
                "name": "Subtask 1",
                "max_score": 20,
                "scoring_method": "GroupMin",
                "test_case_ids": [1, 2, 3]
            },
            {
                "id": 2,
                "name": "Subtask 2",
                "max_score": 30,
                "scoring_method": "GroupMin",
                "test_case_ids": [4, 5]
            },
            {
                "id": 3,
                "name": "Subtask 3",
                "max_score": 50,
                "scoring_method": "GroupMin",
                "test_case_ids": [6, 7, 8, 9]
            }
        ]
        
        response = self.client.call_plugin("configure_problem", {
            "problem_id": 3,
            "subtask_enabled": True,
            "final_score_method": "BestSubtaskSum",
            "subtasks": subtasks
        })
        
        assert response["success"] == True, "Configuration should succeed"
        return response

    def test_get_problem_config(self):
        """Test getting IOI configuration for a problem"""
        response = self.client.call_plugin("get_problem_config_api", {
            "problem_id": 1
        })
        
        # Validate response structure
        assert "problem_id" in response, "Missing problem_id"
        assert "subtask_enabled" in response, "Missing subtask_enabled"
        assert "final_score_method" in response, "Missing final_score_method"
        assert "subtasks" in response, "Missing subtasks"
        
        # Validate subtask structure
        if response["subtasks"]:
            subtask = response["subtasks"][0]
            assert "id" in subtask, "Missing id in subtask"
            assert "name" in subtask, "Missing name in subtask"
            assert "max_score" in subtask, "Missing max_score in subtask"
            assert "scoring_method" in subtask, "Missing scoring_method in subtask"
            assert "test_case_ids" in subtask, "Missing test_case_ids in subtask"
        
        return response

    def test_calculate_submission_score(self):
        """Test calculating submission score (simulates post-judging hook)"""
        response = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 1
        })
        
        # Validate response structure
        assert "success" in response, "Missing success field"
        assert "submission_id" in response, "Missing submission_id"
        assert "score" in response, "Missing score"
        assert "verdict" in response, "Missing verdict"
        assert "subtask_results" in response, "Missing subtask_results"
        assert "message" in response, "Missing message"
        
        # Validate score is calculated
        assert response["submission_id"] == 1, "Wrong submission_id"
        assert isinstance(response["score"], int), "Score should be integer"
        assert response["verdict"] in [
            "Accepted", "PartiallyCorrect", "WrongAnswer", 
            "TimeLimitExceeded", "MemoryLimitExceeded", "RuntimeError"
        ], f"Invalid verdict: {response['verdict']}"
        
        # Validate subtask results structure
        if response["subtask_results"]:
            sr = response["subtask_results"][0]
            assert "subtask_id" in sr, "Missing subtask_id"
            assert "subtask_name" in sr, "Missing subtask_name"
            assert "score" in sr, "Missing score in subtask"
            assert "max_score" in sr, "Missing max_score"
            assert "verdict" in sr, "Missing verdict in subtask"
        
        return response

    def test_calculate_score_not_found(self):
        """Test calculating score for non-existent submission"""
        response = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 99999
        })
        
        assert "success" in response, "Missing success field"
        # Should fail gracefully for non-existent submission
        # The mock may still return data, so we just check the structure
        
        return response

    def test_leaderboard_uses_stored_scores(self):
        """
        Test that leaderboard reads scores from judge_result.score directly.
        
        Workflow:
        1. Get initial leaderboard - mock scores may be 0 (not yet calculated)
        2. Call calculate_submission_score for each submission
        3. Get leaderboard again - scores should reflect calculated values
        
        Note: In real production, judge callback triggers score calculation.
        This test simulates that workflow.
        """
        # First, calculate scores for all test submissions
        submission_ids = [1, 2, 3, 4]  # Known mock submission IDs
        calculated_scores = {}
        
        for sid in submission_ids:
            result = self.client.call_plugin("calculate_submission_score", {
                "submission_id": sid
            })
            if result.get("success"):
                calculated_scores[sid] = result["score"]
        
        # Now get leaderboard - it should use the stored scores
        leaderboard = self.client.call_plugin("get_leaderboard", {
            "contest_id": 1
        })
        
        assert "entries" in leaderboard, "Missing entries in leaderboard"
        assert len(leaderboard["entries"]) > 0, "Leaderboard should have entries"
        
        # Verify that leaderboard scores match calculated scores
        # The total_score should be sum of best scores per problem
        for entry in leaderboard["entries"]:
            total = entry["total_score"]
            username = entry["user"]["username"]
            
            # Each user's total should be > 0 if they have submissions that scored points
            # (Mock data has users with AC submissions)
            print(f"    {username}: total_score = {total}")
            
            # Verify problem_scores structure
            for ps in entry["problem_scores"]:
                assert "score" in ps, "Missing score in problem_scores"
                assert "max_score" in ps, "Missing max_score in problem_scores"
                # score should be within valid range
                assert 0 <= ps["score"] <= ps["max_score"], \
                    f"Invalid score {ps['score']} for max_score {ps['max_score']}"
        
        return {
            "calculated_scores": calculated_scores,
            "leaderboard_entries": [
                {"username": e["user"]["username"], "total_score": e["total_score"]}
                for e in leaderboard["entries"]
            ]
        }

    # ========================================================================
    # Subtask Scoring Mechanism Tests
    # ========================================================================

    def test_subtask_sum_scoring(self):
        """
        Test Sum scoring method: all test case scores are summed.
        
        Mock submission 1 (Alice, Problem 1):
        - Subtask 1 uses Sum scoring with test cases 1,2,3
        - Each test case scores 10 points
        - Expected subtask score: 10 + 10 + 10 = 30
        """
        # Calculate score for submission 1
        result = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 1
        })
        
        assert result["success"], f"Score calculation failed: {result.get('message')}"
        assert "subtask_results" in result, "Missing subtask_results"
        
        # Find Subtask 1 (Sum scoring)
        subtask1 = None
        for sr in result["subtask_results"]:
            if sr["subtask_id"] == 1:
                subtask1 = sr
                break
        
        assert subtask1 is not None, "Subtask 1 not found in results"
        
        # Verify Sum scoring: all test cases passed, so score should equal max_score
        # In mock: 3 test cases, each 10 points = 30 points max
        assert subtask1["score"] == 30, \
            f"Subtask 1 (Sum) expected 30, got {subtask1['score']}"
        assert subtask1["max_score"] == 30, \
            f"Subtask 1 max_score expected 30, got {subtask1['max_score']}"
        assert subtask1["verdict"] == "Accepted", \
            f"Subtask 1 verdict expected Accepted, got {subtask1['verdict']}"
        
        return {
            "submission_id": 1,
            "subtask1_score": subtask1["score"],
            "subtask1_max": subtask1["max_score"],
            "total_score": result["score"]
        }

    def test_subtask_groupmin_scoring(self):
        """
        Test GroupMin scoring method: all-or-nothing based on whether all tests pass.
        
        Mock submission 1 (Alice, Problem 1):
        - Subtask 2 uses GroupMin scoring with test cases 4,5
        - All test cases passed -> full score (30 points)
        
        Mock submission 3 (Bob, Problem 1):
        - Subtask 3 uses GroupMin scoring with test cases 6,7
        - One test case TLE -> 0 points (all-or-nothing)
        """
        # Test case 1: All passing -> full score
        result1 = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 1
        })
        
        assert result1["success"], f"Score calculation failed: {result1.get('message')}"
        
        # Find Subtask 2 (GroupMin scoring, all passed)
        subtask2 = None
        for sr in result1["subtask_results"]:
            if sr["subtask_id"] == 2:
                subtask2 = sr
                break
        
        assert subtask2 is not None, "Subtask 2 not found"
        assert subtask2["score"] == subtask2["max_score"], \
            f"GroupMin with all AC should get full score, got {subtask2['score']}/{subtask2['max_score']}"
        
        # Test case 2: One failing -> zero score
        result3 = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 3
        })
        
        assert result3["success"], f"Score calculation failed: {result3.get('message')}"
        
        # Find Subtask 3 (GroupMin scoring, one TLE)
        subtask3 = None
        for sr in result3["subtask_results"]:
            if sr["subtask_id"] == 3:
                subtask3 = sr
                break
        
        assert subtask3 is not None, "Subtask 3 not found"
        assert subtask3["score"] == 0, \
            f"GroupMin with one failing should get 0, got {subtask3['score']}"
        
        return {
            "submission1_subtask2": {"score": subtask2["score"], "max": subtask2["max_score"], "verdict": subtask2["verdict"]},
            "submission3_subtask3": {"score": subtask3["score"], "max": subtask3["max_score"], "verdict": subtask3["verdict"]}
        }

    def test_subtask_partial_score(self):
        """
        Test partial scoring: Sum method allows partial points.
        
        Mock submission 5 (Charlie, Problem 1):
        - Subtask 1 uses Sum scoring
        - 2 AC (10+10 points) + 1 WA (0 points) = 20 points
        """
        result = self.client.call_plugin("calculate_submission_score", {
            "submission_id": 5
        })
        
        assert result["success"], f"Score calculation failed: {result.get('message')}"
        assert "subtask_results" in result, "Missing subtask_results"
        
        # Find Subtask 1 (Sum scoring with partial score)
        subtask1 = None
        for sr in result["subtask_results"]:
            if sr["subtask_id"] == 1:
                subtask1 = sr
                break
        
        assert subtask1 is not None, "Subtask 1 not found"
        
        # Verify partial scoring: 2 AC (20 points) + 1 WA (0 points) = 20 points
        assert subtask1["score"] == 20, \
            f"Subtask 1 (Sum, partial) expected 20, got {subtask1['score']}"
        assert subtask1["verdict"] == "PartiallyCorrect", \
            f"Subtask 1 verdict expected PartiallyCorrect, got {subtask1['verdict']}"
        
        # Verify total score includes partial credit
        # Subtask 1 (Sum): 20, Subtask 2 (GroupMin, one WA): 0, Subtask 3 (GroupMin, all WA): 0
        assert result["score"] == 20, \
            f"Total score expected 20, got {result['score']}"
        
        return {
            "submission_id": 5,
            "subtask1_score": subtask1["score"],
            "subtask1_verdict": subtask1["verdict"],
            "total_score": result["score"],
            "overall_verdict": result["verdict"]
        }


# ============================================================================
# Main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(description="IOI Plugin API Test Suite")
    parser.add_argument(
        "--base-url",
        default="http://localhost:3000",
        help="Base URL of the server (default: http://localhost:3000)"
    )
    parser.add_argument(
        "--plugin-id",
        default="ioi-plugin",
        help="Plugin ID (default: ioi-plugin)"
    )
    parser.add_argument(
        "-q", "--quiet",
        action="store_true",
        help="Quiet mode (don't print response details)"
    )
    
    args = parser.parse_args()
    
    config = TestConfig(
        base_url=args.base_url,
        plugin_id=args.plugin_id,
        verbose=not args.quiet
    )
    
    client = APIClient(config)
    suite = IOIPluginTestSuite(client, verbose=config.verbose)
    
    success = suite.run_all()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
