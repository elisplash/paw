// Settings: Voice — TTS, Talk Mode, Voice Wake
// ~180 lines

import { gateway } from '../gateway';
import { showToast } from '../components/toast';
import {
  isConnected, esc, formRow, selectInput, toggleSwitch, textInput
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── TTS ─────────────────────────────────────────────────────────────────────

export async function loadVoiceSettings() {
  if (!isConnected()) return;
  const container = $('settings-voice-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading voice config…</p>';

  try {
    const [status, providersResult, talkConf, wakeConf] = await Promise.all([
      gateway.ttsStatus().catch(() => null),
      gateway.ttsProviders().catch(() => ({ providers: [] })),
      gateway.talkConfig().catch(() => null),
      gateway.voicewakeGet().catch(() => ({ triggers: [] })),
    ]);

    container.innerHTML = '';

    // ── TTS Section ──────────────────────────────────────────────────────
    const ttsSection = document.createElement('div');
    ttsSection.innerHTML = '<h3 class="settings-subsection-title">Text-to-Speech</h3>';

    // Enable toggle
    const { container: ttsToggle, checkbox: ttsCb } = toggleSwitch(
      status?.enabled ?? false,
      'Enable TTS'
    );
    ttsCb.onchange = async () => {
      try {
        await gateway.ttsEnable(ttsCb.checked);
        showToast(`TTS ${ttsCb.checked ? 'enabled' : 'disabled'}`, 'success');
      } catch (e: any) { showToast(e.message || String(e), 'error'); }
    };
    ttsSection.appendChild(ttsToggle);

    // Provider select
    const providers = providersResult.providers ?? [];
    if (providers.length > 0) {
      const provRow = formRow('Provider', 'Select TTS voice provider');
      const provOpts = providers.map(p => ({ value: p.id, label: p.name || p.id }));
      const provSel = selectInput(provOpts, status?.provider ?? '');
      provSel.style.maxWidth = '240px';
      provRow.appendChild(provSel);
      ttsSection.appendChild(provRow);

      // Voice select (populated from selected provider)
      const voiceRow = formRow('Voice');
      const voiceSel = document.createElement('select');
      voiceSel.className = 'input';
      voiceSel.style.maxWidth = '240px';

      const populateVoices = () => {
        const prov = providers.find(p => p.id === provSel.value);
        voiceSel.innerHTML = '';
        const voices = prov?.voices ?? [];
        if (voices.length === 0) {
          voiceSel.innerHTML = '<option value="">(no voices)</option>';
        } else {
          for (const v of voices) {
            const opt = document.createElement('option');
            opt.value = v; opt.textContent = v;
            if (v === status?.voice) opt.selected = true;
            voiceSel.appendChild(opt);
          }
        }
      };
      populateVoices();
      provSel.onchange = () => populateVoices();
      voiceRow.appendChild(voiceSel);
      ttsSection.appendChild(voiceRow);

      // Save provider
      const saveProvBtn = document.createElement('button');
      saveProvBtn.className = 'btn btn-sm btn-primary';
      saveProvBtn.textContent = 'Set Provider & Voice';
      saveProvBtn.style.marginTop = '8px';
      saveProvBtn.onclick = async () => {
        try {
          await gateway.ttsSetProvider(provSel.value, voiceSel.value || undefined);
          showToast('TTS provider updated', 'success');
        } catch (e: any) { showToast(e.message || String(e), 'error'); }
      };
      ttsSection.appendChild(saveProvBtn);
    } else {
      const noP = document.createElement('p');
      noP.style.color = 'var(--text-muted)';
      noP.textContent = 'No TTS providers available.';
      ttsSection.appendChild(noP);
    }

    // Test TTS
    const testRow = document.createElement('div');
    testRow.style.cssText = 'margin-top:12px; display:flex; gap:6px; align-items:center; flex-wrap:wrap';
    const testInp = document.createElement('input');
    testInp.type = 'text'; testInp.className = 'input';
    testInp.placeholder = 'Type text to test TTS…'; testInp.style.flex = '1';
    testInp.style.minWidth = '200px';
    const testBtn = document.createElement('button');
    testBtn.className = 'btn btn-sm';
    testBtn.textContent = 'Test';
    testBtn.onclick = async () => {
      const text = testInp.value.trim();
      if (!text) return;
      try {
        const result = await gateway.ttsConvert(text);
        showToast('TTS test sent', 'success');
        if (result.url) window.open(result.url, '_blank');
      } catch (e: any) { showToast(e.message || String(e), 'error'); }
    };
    testRow.appendChild(testInp);
    testRow.appendChild(testBtn);
    ttsSection.appendChild(testRow);

    container.appendChild(ttsSection);

    // ── Talk Mode ────────────────────────────────────────────────────────
    const talkSection = document.createElement('div');
    talkSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Talk Mode (Continuous Voice)</h3>';

    const { container: talkToggle, checkbox: talkCb } = toggleSwitch(
      talkConf?.enabled ?? false,
      'Enable Talk Mode'
    );
    talkCb.onchange = async () => {
      try {
        await gateway.talkMode(talkCb.checked);
        showToast(`Talk mode ${talkCb.checked ? 'enabled' : 'disabled'}`, 'success');
      } catch (e: any) { showToast(e.message || String(e), 'error'); }
    };
    talkSection.appendChild(talkToggle);

    if (talkConf?.wakeWord) {
      const wwRow = formRow('Wake Word');
      const wwSpan = document.createElement('span');
      wwSpan.style.cssText = 'font-family:monospace; color:var(--text-secondary)';
      wwSpan.textContent = talkConf.wakeWord;
      wwRow.appendChild(wwSpan);
      talkSection.appendChild(wwRow);
    }

    if (talkConf?.voice) {
      const vRow = formRow('Voice');
      const vSpan = document.createElement('span');
      vSpan.style.cssText = 'font-family:monospace; color:var(--text-secondary)';
      vSpan.textContent = talkConf.voice;
      vRow.appendChild(vSpan);
      talkSection.appendChild(vRow);
    }

    container.appendChild(talkSection);

    // ── Voice Wake ───────────────────────────────────────────────────────
    const wakeSection = document.createElement('div');
    wakeSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Voice Wake Triggers</h3>';

    const triggers = wakeConf?.triggers ?? [];
    const wakeInp = textInput(triggers.join(', '), 'hey claw, ok claw');
    wakeInp.title = 'Comma-separated list of wake triggers';
    wakeSection.appendChild(wakeInp);

    const wakeDesc = document.createElement('p');
    wakeDesc.style.cssText = 'font-size:12px; color:var(--text-muted); margin:4px 0 0';
    wakeDesc.textContent = 'Comma-separated list of phrases that activate voice input.';
    wakeSection.appendChild(wakeDesc);

    const wakeSaveBtn = document.createElement('button');
    wakeSaveBtn.className = 'btn btn-sm btn-primary';
    wakeSaveBtn.textContent = 'Save Triggers';
    wakeSaveBtn.style.marginTop = '8px';
    wakeSaveBtn.onclick = async () => {
      try {
        const newTriggers = wakeInp.value.split(',').map(s => s.trim()).filter(Boolean);
        await gateway.voicewakeSet(newTriggers);
        showToast('Wake triggers updated', 'success');
      } catch (e: any) { showToast(e.message || String(e), 'error'); }
    };
    wakeSection.appendChild(wakeSaveBtn);

    container.appendChild(wakeSection);

  } catch (e: any) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load voice config: ${esc(String(e))}</p>`;
  }
}

export function initVoiceSettings() {
  // All dynamic
}
