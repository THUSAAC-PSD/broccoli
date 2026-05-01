
pub const SOLUTION_AB_CPP_AC: &str = include_str!("../fixtures/solutions/ab_cpp_ac.cpp");
pub const SOLUTION_AB_CPP_WA: &str = include_str!("../fixtures/solutions/ab_cpp_wa.cpp");
pub const SOLUTION_AB_CPP_TLE: &str = include_str!("../fixtures/solutions/ab_cpp_tle.cpp");
pub const SOLUTION_AB_CPP_MLE: &str = include_str!("../fixtures/solutions/ab_cpp_mle.cpp");
pub const SOLUTION_AB_CPP_RE: &str = include_str!("../fixtures/solutions/ab_cpp_re.cpp");
pub const SOLUTION_AB_CPP_CE: &str = include_str!("../fixtures/solutions/ab_cpp_ce.cpp");
pub const SOLUTION_AB_CPP_IGNCASE: &str = include_str!("../fixtures/solutions/ab_cpp_igncase.cpp");
pub const SOLUTION_AB_PY_AC: &str = include_str!("../fixtures/solutions/ab_py_ac.py");

pub const MULTI_FILE_SOLUTION_CPP: &str = include_str!("../fixtures/multi-file/solution.cpp");
pub const MULTI_FILE_HELPER_HPP: &str = include_str!("../fixtures/multi-file/helper.hpp");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_is_non_empty() {
        for (name, content) in [
            ("ab_cpp_ac", SOLUTION_AB_CPP_AC),
            ("ab_cpp_wa", SOLUTION_AB_CPP_WA),
            ("ab_cpp_tle", SOLUTION_AB_CPP_TLE),
            ("ab_cpp_mle", SOLUTION_AB_CPP_MLE),
            ("ab_cpp_re", SOLUTION_AB_CPP_RE),
            ("ab_cpp_ce", SOLUTION_AB_CPP_CE),
            ("ab_cpp_igncase", SOLUTION_AB_CPP_IGNCASE),
            ("ab_py_ac", SOLUTION_AB_PY_AC),
            ("multi_file_solution", MULTI_FILE_SOLUTION_CPP),
            ("multi_file_helper", MULTI_FILE_HELPER_HPP),
        ] {
            assert!(!content.trim().is_empty(), "fixture {name} is empty");
        }
    }

    #[test]
    fn ce_fixture_is_actually_malformed() {
        assert!(!SOLUTION_AB_CPP_CE.contains("return"));
        assert!(SOLUTION_AB_CPP_CE.contains("int main("));
    }

    #[test]
    fn igncase_fixture_prints_lowercase() {
        let cout_line = SOLUTION_AB_CPP_IGNCASE
            .lines()
            .find(|l| l.contains("std::cout"))
            .expect("igncase fixture must have a std::cout line");
        assert!(cout_line.contains("\"yes"));
        assert!(!cout_line.contains("\"YES"));
    }

    #[test]
    fn multi_file_solution_includes_helper() {
        assert!(MULTI_FILE_SOLUTION_CPP.contains("#include \"helper.hpp\""));
        assert!(MULTI_FILE_HELPER_HPP.contains("inline int add"));
    }
}
