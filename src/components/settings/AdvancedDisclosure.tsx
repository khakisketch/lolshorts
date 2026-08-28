import { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight } from "lucide-react";

interface AdvancedDisclosureProps {
  /** 열었을 때 나오는 개별 항목들 — 기존 상세 설정 컴포넌트 그대로. */
  children: ReactNode;
  /** 「고급 설정」 아래 한 줄로 무엇이 들어 있는지. */
  summary?: string;
  testId?: string;
}

/**
 * 고급 설정을 접어 둔다.
 *
 * 카테고리를 나눈 뒤에도 각 칸이 여전히 세 화면분이었다. 카드 + 드롭다운으로
 * 바꾼 항목(프레임·비트레이트)이 **바로 아래 상세 설정에 그대로 또 나오기**
 * 때문이다 — 같은 값을 두 번 보여주면 어느 쪽이 진짜인지 알 수 없고, 결국
 * 스크롤로 둘 다 훑게 된다.
 *
 * 지우지 않고 접는다. 개별 항목은 같은 상태를 보므로(위에서 바꾸면 아래도 바뀐다)
 * 필요한 사람만 펼치면 된다.
 *
 * `<details>` 를 쓴 이유: 상태 관리 없이 키보드(Enter/Space)·스크린리더가
 * 그대로 동작하고, 브라우저가 열림 상태를 접근성 트리에 알려준다.
 */
export function AdvancedDisclosure({
  children,
  summary,
  testId = "advanced-disclosure",
}: AdvancedDisclosureProps) {
  const { t } = useTranslation();

  return (
    <details
      data-testid={testId}
      className="group rounded-lg border border-white/5"
    >
      <summary className="flex min-h-[44px] cursor-pointer list-none items-center gap-2 px-4 py-3 text-sm text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden">
        <ChevronRight
          className="h-4 w-4 shrink-0 transition-transform group-open:rotate-90"
          aria-hidden="true"
        />
        <span className="font-medium text-foreground">
          {t("settings.advanced.title")}
        </span>
        <span className="min-w-0 truncate" style={{ wordBreak: "keep-all" }}>
          {summary ?? t("settings.advanced.defaultSummary")}
        </span>
      </summary>
      <div className="space-y-4 border-t border-white/5 p-4">{children}</div>
    </details>
  );
}
