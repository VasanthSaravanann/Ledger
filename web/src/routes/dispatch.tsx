import { createFileRoute } from "@tanstack/react-router";
import { canonical } from "@/lib/site";
import { DispatchWorkspace } from "@/components/support-workspaces";
export const Route = createFileRoute("/dispatch")({
  head: () => ({
    meta: [
      { title: "Dispatch queue | ledger" },
      {
        name: "description",
        content: "Prepare candidate email drafts and export bundles with SHA-256 manifests.",
      },
      { property: "og:title", content: "Dispatch queue | ledger" },
      {
        property: "og:description",
        content: "Prepare candidate email drafts and export bundles with SHA-256 manifests.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
      ...canonical("/dispatch").meta,
    ],
    links: canonical("/dispatch").links,
  }),
  component: DispatchWorkspace,
});
