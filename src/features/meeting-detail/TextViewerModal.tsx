import { Button } from "@/components/ui/Button";
import { IconBadge, type IconName } from "@/components/ui/Icon";

/** 議事録やまとめの全文を読むためのモーダル。 */
export function TextViewerModal({
  title,
  icon,
  iconTone,
  content,
  onClose,
}: {
  title: string;
  icon: IconName;
  iconTone: "blue" | "green" | "purple" | "orange";
  content: string;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={title}>
      <div className="modal modal--wide">
        <div className="row gap-12">
          <IconBadge name={icon} tone={iconTone} />
          <h2 className="grow">{title}</h2>
          <Button variant="ghost" icon="close" onClick={onClose} aria-label="閉じる">
            閉じる
          </Button>
        </div>
        <div className="viewer">{content}</div>
      </div>
    </div>
  );
}
