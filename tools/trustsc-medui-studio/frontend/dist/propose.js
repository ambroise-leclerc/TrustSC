// "Propose change" dialog (ADR-022 wave S15): turns the canvas editor's in-memory document into
// a reviewable pull request without the manager ever touching git. A manager's edit becomes a
// branch + commit + PR on the server side (`POST /api/proposals`); CI's `--verify-ui` and human
// review remain the regulatory gate, unchanged.
import { ApiError, proposeChange } from "./api.js";
import { changeCount, describeChange, diffScreens } from "./changes.js";
import { el } from "./dom.js";
/** Wires the modal's three passive close paths (backdrop click, Escape, and — via the returned
 * `close()` — every explicit button) to a single teardown, so the document-level `keydown`
 * listener is removed no matter which path the dialog actually closes through. Returns `close()`
 * for the caller's own buttons (Cancel, the success screen's Close) to invoke. */
function setUpDialogClose(backdrop, dialog) {
    const onKey = (event) => {
        if (event.key === "Escape") {
            close();
        }
    };
    const close = () => {
        document.removeEventListener("keydown", onKey);
        backdrop.remove();
    };
    document.addEventListener("keydown", onKey);
    backdrop.addEventListener("click", (event) => {
        if (event.target === backdrop) {
            close();
        }
    });
    dialog.addEventListener("click", (event) => event.stopPropagation());
    return close;
}
/** Opens the propose-change modal, prefilled from the diff against the loaded file. Handles the
 * three server-side rejections a submit can hit (`stale_base`, `comment_loss`, `uncompilable`)
 * and the success screen with the resulting PR link (or a warning when none could be opened,
 * e.g. no GitHub remote configured on the server). */
export function openProposeDialog(options) {
    const { screenId, editor, baseSourceSha256 } = options;
    const diff = diffScreens(editor.getInitialScreen(), editor.getScreen());
    const count = changeCount(diff);
    if (count === 0) {
        return; // The button is disabled in this state; a defensive no-op if invoked anyway.
    }
    const defaultTitle = `MedUI Studio: update ${screenId.split("/").pop()}`;
    const defaultDescription = [
        ...diff.entries.map((entry) => `- ${describeChange(entry)}`),
        ...(diff.screenChanged ? ["- screen layout/surface changed"] : []),
    ].join("\n");
    const titleInput = el("input", { type: "text", class: "propose-dialog__control", value: defaultTitle });
    const descriptionInput = el("textarea", { class: "propose-dialog__control propose-dialog__description", rows: "6" });
    descriptionInput.value = defaultDescription;
    const status = el("p", { class: "propose-dialog__status" });
    const submit = el("button", { type: "button", class: "propose-dialog__submit" }, ["Create pull request"]);
    const cancel = el("button", { type: "button", class: "propose-dialog__cancel" }, ["Cancel"]);
    const body = el("div", { class: "propose-dialog__body" }, [
        el("label", { class: "propose-dialog__field" }, [el("span", {}, ["Title"]), titleInput]),
        el("label", { class: "propose-dialog__field" }, [el("span", {}, ["Description"]), descriptionInput]),
        status,
    ]);
    const dialog = el("div", { class: "propose-dialog" }, [
        el("h2", {}, ["Propose change"]),
        el("p", { class: "propose-dialog__hint" }, [
            `${count} change${count === 1 ? "" : "s"} vs. the loaded file. CI's --verify-ui and human review still gate the merge.`,
        ]),
        body,
        el("div", { class: "propose-dialog__actions" }, [cancel, submit]),
    ]);
    const backdrop = el("div", { class: "propose-backdrop" }, [dialog]);
    const close = setUpDialogClose(backdrop, dialog);
    cancel.addEventListener("click", close);
    document.body.append(backdrop);
    titleInput.focus();
    titleInput.select();
    let allowCommentLoss = false;
    const showSuccess = (result) => {
        const link = result.prUrl
            ? el("p", {}, [el("a", { href: result.prUrl, target: "_blank", rel: "noopener" }, [result.prUrl])])
            : el("p", { class: "propose-dialog__warning" }, [result.warning ?? "Branch pushed; no PR link available."]);
        const closeButton = el("button", { type: "button", class: "propose-dialog__submit" }, ["Close"]);
        closeButton.addEventListener("click", close);
        body.replaceChildren(el("p", {}, [`Branch ${result.branch} pushed.`]), link, ...(result.prUrl && result.warning ? [el("p", { class: "propose-dialog__warning" }, [result.warning])] : []));
        dialog.querySelector(".propose-dialog__actions")?.replaceChildren(closeButton);
    };
    const submitProposal = async () => {
        submit.setAttribute("disabled", "disabled");
        status.textContent = "";
        status.className = "propose-dialog__status";
        try {
            const result = await proposeChange({
                screenId,
                screen: editor.getScreen(),
                baseSourceSha256,
                title: titleInput.value.trim() || defaultTitle,
                description: descriptionInput.value,
                allowCommentLoss,
            });
            showSuccess(result);
        }
        catch (error) {
            submit.removeAttribute("disabled");
            if (error instanceof ApiError && error.code === "stale_base") {
                status.className = "propose-dialog__status propose-dialog__status--error";
                const reload = el("button", { type: "button", class: "propose-dialog__submit" }, ["Reload screen"]);
                reload.addEventListener("click", () => window.location.reload());
                status.replaceChildren("The screen changed upstream since it was loaded. Reload and re-apply your edit before proposing again.", reload);
                return;
            }
            if (error instanceof ApiError && error.code === "comment_loss" && !allowCommentLoss) {
                status.className = "propose-dialog__status propose-dialog__status--error";
                const confirmButton = el("button", { type: "button", class: "propose-dialog__submit" }, [
                    "Propose without comments",
                ]);
                confirmButton.addEventListener("click", () => {
                    allowCommentLoss = true;
                    void submitProposal();
                });
                status.replaceChildren("The committed file has // comments the canonical serializer does not preserve. Proposing will drop them.", confirmButton);
                return;
            }
            status.className = "propose-dialog__status propose-dialog__status--error";
            status.textContent = error instanceof ApiError ? error.message : String(error);
        }
    };
    submit.addEventListener("click", () => void submitProposal());
}
