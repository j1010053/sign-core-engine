import { createContext, useCallback, useContext, useEffect, useRef } from "react";

export type SaveDirtyDraft = () => Promise<void>;

type RegisterDirtyDraft = (key: string, save: SaveDirtyDraft | null) => void;

const DirtyDraftContext = createContext<RegisterDirtyDraft>(() => undefined);

export const DirtyDraftProvider = DirtyDraftContext.Provider;

/**
 * Registers page-local editor text with the shell while it differs from the
 * last Rust-backed state. The stable wrapper lets the page replace its save
 * callback without churning the shell registry on every keystroke.
 */
export function useDirtyDraft(
  key: string,
  dirty: boolean,
  saveDraft: SaveDirtyDraft,
) {
  const register = useContext(DirtyDraftContext);
  const saveRef = useRef(saveDraft);
  useEffect(() => {
    saveRef.current = saveDraft;
  }, [saveDraft]);
  const saveCurrent = useCallback(() => saveRef.current(), []);

  useEffect(() => {
    register(key, dirty ? saveCurrent : null);
    return () => register(key, null);
  }, [dirty, key, register, saveCurrent]);
}
