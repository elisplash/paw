// Content Studio — orchestration + public API

import { saveDoc, deleteDoc } from '../../db';
import { pawEngine } from '../../engine';
import { $, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { appState } from '../../state/index';
import { loadContentDocs, createNewDoc, getActiveDocId, setActiveDocId } from './molecules';

export { loadContentDocs };

export function initContent() {
  $('content-new-doc')?.addEventListener('click', createNewDoc);
  $('content-create-first')?.addEventListener('click', createNewDoc);

  $('content-save')?.addEventListener('click', async () => {
    const docId = getActiveDocId();
    if (!docId) return;
    const title = ($('content-title') as HTMLInputElement).value.trim() || 'Untitled';
    const content = ($('content-body') as HTMLTextAreaElement).value;
    const contentType = ($('content-type') as HTMLSelectElement).value;
    await saveDoc({ id: docId, title, content, content_type: contentType });
    const wordCount = $('content-word-count');
    if (wordCount) wordCount.textContent = `${content.split(/\s+/).filter(Boolean).length} words`;
    loadContentDocs();
  });

  $('content-body')?.addEventListener('input', () => {
    const body = $('content-body') as HTMLTextAreaElement;
    const wordCount = $('content-word-count');
    if (wordCount && body) {
      wordCount.textContent = `${body.value.split(/\s+/).filter(Boolean).length} words`;
    }
  });

  $('content-ai-improve')?.addEventListener('click', async () => {
    const docId = getActiveDocId();
    if (!docId || !appState.wsConnected) {
      showToast('Not connected', 'error');
      return;
    }
    const bodyEl = $('content-body') as HTMLTextAreaElement;
    const body = bodyEl?.value.trim();
    if (!body) return;

    const btn = $('content-ai-improve') as HTMLButtonElement | null;
    if (btn) btn.disabled = true;
    showToast('AI improving your text…', 'info');

    try {
      const result = await pawEngine.chatSend(
        'paw-improve',
        `Improve this text. Return only the improved version, no explanations:\n\n${body}`,
      );
      const text = (result as unknown as Record<string, unknown>).text as string | undefined;
      if (text && bodyEl) {
        bodyEl.value = text;
        showToast('Text improved!', 'success');
      } else {
        showToast('Agent returned no text', 'error');
      }
    } catch (e) {
      showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      if (btn) btn.disabled = false;
    }
  });

  $('content-delete-doc')?.addEventListener('click', async () => {
    const docId = getActiveDocId();
    if (!docId) return;
    if (!(await confirmModal('Delete this document?'))) return;
    await deleteDoc(docId);
    setActiveDocId(null);
    loadContentDocs();
  });
}
