export interface IcpcContestInfoResponse {
  penalty_minutes: number;
  count_compile_error: boolean;
  show_test_details: boolean;
}

export interface ProblemCell {
  attempts: number;
  solved: boolean;
  time?: number;
  penalty?: number;
  first_solve?: boolean;
}

export interface StandingsEntry {
  rank: number;
  user_id: number;
  username: string;
  solved: number;
  penalty: number;
  problems: Record<string, ProblemCell>;
}

export interface StandingsResponse {
  phase: 'before' | 'during' | 'after';
  penalty_minutes: number;
  problem_labels: string[];
  rows: StandingsEntry[];
}
