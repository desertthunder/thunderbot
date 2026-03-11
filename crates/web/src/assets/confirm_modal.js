document.addEventListener("alpine:init", () => {
  Alpine.data("tbConfirmModal", () => ({
    title: "Confirm action",
    message: "Are you sure you want to continue?",
    confirmLabel: "Confirm",
    cancelLabel: "Cancel",
    confirmVariant: "default",
    onConfirm: null,
    init() {
      this.$refs.closeButton.addEventListener("click", (event) => {
        event.preventDefault();
        this.cancel();
      });

      this.$refs.cancelButton.addEventListener("click", (event) => {
        event.preventDefault();
        this.cancel();
      });

      this.$refs.confirmButton.addEventListener("click", (event) => {
        event.preventDefault();
        this.confirm();
      });

      this.$refs.dialog.addEventListener("cancel", (event) => {
        event.preventDefault();
        this.cancel();
      });

      this.$refs.dialog.addEventListener("click", (event) => {
        if (event.target === this.$refs.dialog) {
          this.cancel();
        }
      });

      window.addEventListener("tb-confirm:open", (event) => {
        this.open(event.detail || {});
      });
    },
    open(options = {}) {
      this.title = options.title || "Confirm action";
      this.message = options.message || "Are you sure you want to continue?";
      this.confirmLabel = options.confirmLabel || "Confirm";
      this.cancelLabel = options.cancelLabel || "Cancel";
      this.confirmVariant = options.confirmVariant || "default";
      this.onConfirm = typeof options.onConfirm === "function" ? options.onConfirm : null;

      this.$refs.titleEl.textContent = this.title;
      this.$refs.messageEl.textContent = this.message;
      this.$refs.confirmButton.textContent = this.confirmLabel;
      this.$refs.cancelButton.textContent = this.cancelLabel;
      this.$refs.confirmButton.dataset.variant = this.confirmVariant;

      if (!this.$refs.dialog.open) {
        this.$refs.dialog.showModal();
      }

      this.$nextTick(() => {
        this.$refs.confirmButton.focus();
      });
    },
    close() {
      if (this.$refs.dialog.open) {
        this.$refs.dialog.close();
      }
    },
    cancel() {
      this.onConfirm = null;
      this.close();
    },
    confirm() {
      const callback = this.onConfirm;
      this.onConfirm = null;
      this.close();
      if (callback) {
        callback();
      }
    },
  }));
});

document.body.addEventListener("htmx:confirm", (event) => {
  const detail = event.detail || {};
  const rawSource = detail.elt || detail.target || event.target || null;
  const source =
    rawSource && typeof rawSource.closest === "function" ? rawSource.closest("[data-confirm-modal]") : null;
  if (!source) {
    return;
  }

  event.preventDefault();

  const message =
    detail.question || source.getAttribute("data-confirm-message") || "Are you sure you want to continue?";
  const title = source.getAttribute("data-confirm-title") || "Confirm action";
  const confirmLabel = source.getAttribute("data-confirm-label") || "Confirm";
  const cancelLabel = source.getAttribute("data-confirm-cancel-label") || "Cancel";
  const confirmVariant = source.getAttribute("data-confirm-variant") || "default";

  window.dispatchEvent(
    new CustomEvent("tb-confirm:open", {
      detail: {
        title,
        message,
        confirmLabel,
        cancelLabel,
        confirmVariant,
        onConfirm: () => {
          if (typeof detail.issueRequest === "function") {
            detail.issueRequest(true);
          }
        },
      },
    }),
  );
});
