<script setup lang="ts">
import { onMounted, ref } from "vue";

import { getFoundationStatus } from "./api/generated";
import FoundationStatusPanel from "./components/FoundationStatusPanel.vue";
import type { FoundationStatusView } from "./foundation-status";

const phase = ref<FoundationStatusView>({ state: "loading" });

async function loadFoundationStatus(): Promise<void> {
  phase.value = { state: "loading" };

  try {
    const status = await getFoundationStatus();
    phase.value = { state: "ready", status };
  } catch {
    phase.value = { state: "error" };
  }
}

onMounted(loadFoundationStatus);
</script>

<template>
  <main>
    <header>
      <p class="eyebrow">Phase 00 development shell</p>
      <h1>eutheto</h1>
      <p class="lede">A local-first foundation for constraint planning.</p>
    </header>

    <FoundationStatusPanel :phase="phase" @retry="loadFoundationStatus" />
  </main>
</template>
