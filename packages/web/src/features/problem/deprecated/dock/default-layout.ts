import { Orientation } from 'dockview-core';
import type { SerializedDockview } from 'dockview-react';

import {
  PANEL_CODE_EDITOR,
  PANEL_PROBLEM_STATEMENT,
  PANEL_SUBMISSIONS,
} from './panel-registry';

export function getDefaultLayout(): SerializedDockview {
  return {
    grid: {
      root: {
        type: 'branch',
        data: [
          {
            type: 'leaf',
            data: {
              views: [PANEL_PROBLEM_STATEMENT],
              activeView: PANEL_PROBLEM_STATEMENT,
              id: 'group-statement',
            },
            size: 400,
          },
          {
            type: 'branch',
            data: [
              {
                type: 'leaf',
                data: {
                  views: [PANEL_CODE_EDITOR],
                  activeView: PANEL_CODE_EDITOR,
                  id: 'group-editor',
                },
                size: 700,
              },
              {
                type: 'leaf',
                data: {
                  views: [PANEL_SUBMISSIONS],
                  activeView: PANEL_SUBMISSIONS,
                  id: 'group-submissions',
                },
                size: 300,
              },
            ],
            size: 600,
          },
        ],
        size: 1000,
      },
      width: 1000,
      height: 1000,
      orientation: Orientation.HORIZONTAL,
    },
    panels: {
      [PANEL_PROBLEM_STATEMENT]: {
        id: PANEL_PROBLEM_STATEMENT,
        contentComponent: PANEL_PROBLEM_STATEMENT,
        title: 'Problem',
      },
      [PANEL_CODE_EDITOR]: {
        id: PANEL_CODE_EDITOR,
        contentComponent: PANEL_CODE_EDITOR,
        title: 'Code',
      },
      [PANEL_SUBMISSIONS]: {
        id: PANEL_SUBMISSIONS,
        contentComponent: PANEL_SUBMISSIONS,
        title: 'Submissions',
      },
    },
    activeGroup: 'group-editor',
  };
}
