<script setup lang="ts">
import type { FoundationStatusView } from "../foundation-status";

defineProps<{ readonly phase: FoundationStatusView }>();
defineEmits<{ retry: [] }>();
</script>

<template>
  <section class="status-card" aria-labelledby="foundation-heading">
    <h2 id="foundation-heading">Application foundation</h2>

    <p v-if="phase.state === 'loading'" role="status" aria-live="polite">
      Checking the local application foundation…
    </p>

    <template v-else-if="phase.state === 'ready'">
      <p role="status" aria-live="polite">The local application foundation is available.</p>
      <dl>
        <div>
          <dt>Capability</dt>
          <dd>Phase 00 repository foundation</dd>
        </div>
        <div>
          <dt>Status schema</dt>
          <dd>Version {{ phase.status.schemaVersion }}</dd>
        </div>
      </dl>
      <p class="boundary-note">
        Domain planning and solver features are not part of this development shell.
      </p>
    </template>

    <template v-else>
      <p role="alert">The local application foundation could not be reached.</p>
      <button type="button" @click="$emit('retry')">Try again</button>
    </template>
  </section>
</template>
