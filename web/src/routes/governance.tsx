import { createFileRoute } from "@tanstack/react-router";
import { canonical } from "@/lib/site";
import { GovernanceWorkspace } from "@/components/support-workspaces";
export const Route = createFileRoute("/governance")({
  head: () => ({
    meta: [
      { title: "Governance | ledger" },
      {
        name: "description",
        content: "Review recruitment artefacts and attest the operational checklist.",
      },
      { property: "og:title", content: "Governance | ledger" },
      {
        property: "og:description",
        content: "Review recruitment artefacts and attest the operational checklist.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
      ...canonical("/governance").meta,
    ],
    links: canonical("/governance").links,
  }),
  component: GovernanceWorkspace,
});
