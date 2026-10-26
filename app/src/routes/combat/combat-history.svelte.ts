// Undo/redo history of the combat tracker, kept outside the view so it
// survives switching to Encounters, Parties or another page and back.

class CombatHistory {
  /** Saved tracker states (JSON), newest last. */
  undo = $state<string[]>([]);
  redo = $state<string[]>([]);
  /** The state the tracker was last saved in. */
  saved = '';
}

export const history = new CombatHistory();
