<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";

import ProjectHome from "./components/ProjectHome.vue";
import { createProjectHomeController } from "./project-home";

const home = createProjectHomeController();

onMounted(async () => {
  await home.startEventListeners();
  await home.load();
});
onUnmounted(() => {
  void home.dispose();
});
</script>

<template>
  <main>
    <header class="app-header">
      <div>
        <p class="eyebrow">Phase 01 · local-first workspace</p>
        <h1>Eutheto</h1>
      </div>
      <div>
        <p class="lede">Plan carefully. Keep the authoritative work on your machine.</p>
        <p class="boundary-note">
          Solver features and the <code>.eutheto</code> extension remain provisional development
          capabilities.
        </p>
      </div>
    </header>

    <ProjectHome :home="home" />
  </main>
</template>
