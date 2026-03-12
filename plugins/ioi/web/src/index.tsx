// Admin config components
export { ScoringModeSelector } from './ScoringModeSelector';
export { SubtaskEditor } from './SubtaskEditor';
export { TokenConfigPanel } from './TokenConfigPanel';

// Contestant-facing components
export { IoiContestInfo } from './IoiContestInfo';
export { IoiScoreboard } from './IoiScoreboard';
export { IoiSubmissionResult } from './IoiSubmissionResult';
export { TokenPanel } from './TokenPanel';

export function onInit() {
  console.log('IOI contest frontend plugin loaded');
}
