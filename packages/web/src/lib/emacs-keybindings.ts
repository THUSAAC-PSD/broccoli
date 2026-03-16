/**
 * Native Emacs keybindings for Monaco Editor.
 *
 * This replaces the `monaco-emacs` npm package which is incompatible with Vite/ESM
 * (it imports Monaco's AMD bundle which calls `define()`).
 */
import type * as monacoNs from 'monaco-editor';

interface EmacsState {
  disposables: monacoNs.IDisposable[];
  markPosition: monacoNs.IPosition | null;
  killRing: string[];
}

/**
 * Activates Emacs keybindings on the given editor.
 * Returns a disposable that removes all bindings when called.
 */
export function activateEmacsMode(
  editor: monacoNs.editor.IStandaloneCodeEditor,
  monaco: typeof monacoNs,
): { dispose: () => void } {
  const state: EmacsState = {
    disposables: [],
    markPosition: null,
    killRing: [],
  };

  const { KeyMod, KeyCode } = monaco;
  const Ctrl = KeyMod.WinCtrl;
  const Alt = KeyMod.Alt;

  const editorId = editor.getId();

  function bind(keybinding: number, handler: () => void): void {
    const action = editor.addAction({
      id: `emacs.${editorId}.${keybinding}`,
      label: '',
      keybindings: [keybinding],
      run: handler,
    });
    if (action) state.disposables.push(action);
  }

  function getModel() {
    return editor.getModel();
  }

  function getPos(): monacoNs.IPosition {
    return editor.getPosition()!;
  }

  function setPos(line: number, col: number) {
    editor.setPosition({ lineNumber: line, column: col });
    editor.revealPosition({ lineNumber: line, column: col });
  }

  function setSelection(
    startLine: number,
    startCol: number,
    endLine: number,
    endCol: number,
  ) {
    editor.setSelection({
      startLineNumber: startLine,
      startColumn: startCol,
      endLineNumber: endLine,
      endColumn: endCol,
    });
  }

  function killText(text: string) {
    state.killRing.push(text);
    if (state.killRing.length > 60) state.killRing.shift();
    navigator.clipboard.writeText(text).catch(() => {});
  }

  // Ctrl+F: Forward char
  bind(Ctrl | KeyCode.KeyF, () => {
    editor.trigger('emacs', 'cursorRight', null);
  });

  // Ctrl+B: Backward char
  bind(Ctrl | KeyCode.KeyB, () => {
    editor.trigger('emacs', 'cursorLeft', null);
  });

  // Ctrl+N: Next line
  bind(Ctrl | KeyCode.KeyN, () => {
    editor.trigger('emacs', 'cursorDown', null);
  });

  // Ctrl+P: Previous line
  bind(Ctrl | KeyCode.KeyP, () => {
    editor.trigger('emacs', 'cursorUp', null);
  });

  // Ctrl+A: Beginning of line
  bind(Ctrl | KeyCode.KeyA, () => {
    editor.trigger('emacs', 'cursorHome', null);
  });

  // Ctrl+E: End of line
  bind(Ctrl | KeyCode.KeyE, () => {
    editor.trigger('emacs', 'cursorEnd', null);
  });

  // Alt+F: Forward word
  bind(Alt | KeyCode.KeyF, () => {
    editor.trigger('emacs', 'cursorWordEndRight', null);
  });

  // Alt+B: Backward word
  bind(Alt | KeyCode.KeyB, () => {
    editor.trigger('emacs', 'cursorWordStartLeft', null);
  });

  // Alt+< (Alt+Shift+Comma): Beginning of buffer
  bind(Alt | KeyMod.Shift | KeyCode.Comma, () => {
    setPos(1, 1);
  });

  // Alt+> (Alt+Shift+Period): End of buffer
  bind(Alt | KeyMod.Shift | KeyCode.Period, () => {
    const model = getModel();
    if (!model) return;
    const lastLine = model.getLineCount();
    const lastCol = model.getLineMaxColumn(lastLine);
    setPos(lastLine, lastCol);
  });

  // Ctrl+D: Delete char forward
  bind(Ctrl | KeyCode.KeyD, () => {
    editor.trigger('emacs', 'deleteRight', null);
  });

  // Alt+D: Delete word forward
  bind(Alt | KeyCode.KeyD, () => {
    const model = getModel();
    if (!model) return;
    const pos = getPos();
    const wordAtPos = model.getWordAtPosition(pos);
    let endCol: number;
    if (wordAtPos && pos.column <= wordAtPos.endColumn) {
      endCol = wordAtPos.endColumn;
    } else {
      const lineContent = model.getLineContent(pos.lineNumber);
      const after = lineContent.substring(pos.column - 1);
      const match = /^\s*\S+/.exec(after);
      endCol = match
        ? pos.column + match[0].length
        : model.getLineMaxColumn(pos.lineNumber);
    }
    const range = {
      startLineNumber: pos.lineNumber,
      startColumn: pos.column,
      endLineNumber: pos.lineNumber,
      endColumn: endCol,
    };
    const text = model.getValueInRange(range);
    if (text) {
      killText(text);
      editor.executeEdits('emacs', [{ range, text: '' }]);
    }
  });

  // Ctrl+K: Kill to end of line
  bind(Ctrl | KeyCode.KeyK, () => {
    const model = getModel();
    if (!model) return;
    const pos = getPos();
    const lineContent = model.getLineContent(pos.lineNumber);
    const afterCursor = lineContent.substring(pos.column - 1);

    if (afterCursor.length === 0) {
      // At end of line, kill the newline (join with next line)
      if (pos.lineNumber < model.getLineCount()) {
        const range = {
          startLineNumber: pos.lineNumber,
          startColumn: pos.column,
          endLineNumber: pos.lineNumber + 1,
          endColumn: 1,
        };
        killText('\n');
        editor.executeEdits('emacs', [{ range, text: '' }]);
      }
    } else {
      const range = {
        startLineNumber: pos.lineNumber,
        startColumn: pos.column,
        endLineNumber: pos.lineNumber,
        endColumn: model.getLineMaxColumn(pos.lineNumber),
      };
      killText(afterCursor);
      editor.executeEdits('emacs', [{ range, text: '' }]);
    }
  });

  // Ctrl+Y: Yank (paste from kill ring)
  bind(Ctrl | KeyCode.KeyY, () => {
    const text =
      state.killRing.length > 0
        ? state.killRing[state.killRing.length - 1]
        : null;
    if (text) {
      editor.trigger('emacs', 'type', { text });
    }
  });

  // Ctrl+W: Kill region (cut selection)
  bind(Ctrl | KeyCode.KeyW, () => {
    const selection = editor.getSelection();
    if (!selection || selection.isEmpty()) return;
    const model = getModel();
    if (!model) return;
    const text = model.getValueInRange(selection);
    killText(text);
    editor.executeEdits('emacs', [{ range: selection, text: '' }]);
  });

  // Alt+W: Copy region (copy selection)
  bind(Alt | KeyCode.KeyW, () => {
    const selection = editor.getSelection();
    if (!selection || selection.isEmpty()) return;
    const model = getModel();
    if (!model) return;
    const text = model.getValueInRange(selection);
    killText(text);
  });

  // Ctrl+Space: Set mark
  bind(Ctrl | KeyCode.Space, () => {
    state.markPosition = getPos();
  });

  // Mark selection via cursor movement
  let updatingSelection = false;
  const cursorDisposable = editor.onDidChangeCursorPosition((e) => {
    if (!state.markPosition) return;
    if (updatingSelection) return;
    updatingSelection = true;
    const mark = state.markPosition;
    setSelection(
      mark.lineNumber,
      mark.column,
      e.position.lineNumber,
      e.position.column,
    );
    updatingSelection = false;
  });
  state.disposables.push(cursorDisposable);

  // Ctrl+G: Cancel (deactivate mark, cancel selection)
  bind(Ctrl | KeyCode.KeyG, () => {
    state.markPosition = null;
    const pos = getPos();
    editor.setSelection({
      startLineNumber: pos.lineNumber,
      startColumn: pos.column,
      endLineNumber: pos.lineNumber,
      endColumn: pos.column,
    });
  });

  // Ctrl+/: Undo
  bind(Ctrl | KeyCode.Slash, () => {
    editor.trigger('emacs', 'undo', null);
  });

  // Ctrl+L: Recenter (scroll cursor to center)
  bind(Ctrl | KeyCode.KeyL, () => {
    editor.trigger('emacs', 'revealLine', {
      lineNumber: getPos().lineNumber,
      at: 'center',
    });
  });

  // Ctrl+O: Open line (insert newline without moving cursor)
  bind(Ctrl | KeyCode.KeyO, () => {
    const pos = getPos();
    editor.executeEdits('emacs', [
      {
        range: {
          startLineNumber: pos.lineNumber,
          startColumn: pos.column,
          endLineNumber: pos.lineNumber,
          endColumn: pos.column,
        },
        text: '\n',
      },
    ]);
    setPos(pos.lineNumber, pos.column);
  });

  return {
    dispose() {
      for (const d of state.disposables) d.dispose();
      state.disposables.length = 0;
      state.markPosition = null;
      state.killRing.length = 0;
    },
  };
}
