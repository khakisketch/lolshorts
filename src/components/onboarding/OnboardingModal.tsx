import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Gamepad2,
  Video,
  Wand2,
  Upload,
  Sparkles,
  ArrowRight,
  ArrowLeft,
  Check,
  HardDrive,
  Play,
} from 'lucide-react';

const ONBOARDING_KEY = 'lolshorts_onboarding_completed';

interface OnboardingStep {
  id: string;
  icon: React.ReactNode;
  titleKey: string;
  descriptionKey: string;
  tips?: string[];
}

const steps: OnboardingStep[] = [
  {
    id: 'welcome',
    icon: <Sparkles className="w-16 h-16 text-primary" />,
    titleKey: 'onboarding.welcome.title',
    descriptionKey: 'onboarding.welcome.description',
  },
  {
    id: 'connect',
    icon: <Gamepad2 className="w-16 h-16 text-blue-500" />,
    titleKey: 'onboarding.connect.title',
    descriptionKey: 'onboarding.connect.description',
    tips: ['onboarding.connect.tip1', 'onboarding.connect.tip2'],
  },
  {
    id: 'storage',
    icon: <HardDrive className="w-16 h-16 text-orange-500" />,
    titleKey: 'onboarding.storage.title',
    descriptionKey: 'onboarding.storage.description',
    tips: ['onboarding.storage.tip1', 'onboarding.storage.tip2'],
  },
  {
    id: 'record',
    icon: <Video className="w-16 h-16 text-red-500" />,
    titleKey: 'onboarding.record.title',
    descriptionKey: 'onboarding.record.description',
    tips: ['onboarding.record.tip1', 'onboarding.record.tip2'],
  },
  {
    id: 'replay',
    icon: <Play className="w-16 h-16 text-cyan-500" />,
    titleKey: 'onboarding.replay.title',
    descriptionKey: 'onboarding.replay.description',
    tips: ['onboarding.replay.tip1', 'onboarding.replay.tip2'],
  },
  {
    id: 'edit',
    icon: <Wand2 className="w-16 h-16 text-purple-500" />,
    titleKey: 'onboarding.edit.title',
    descriptionKey: 'onboarding.edit.description',
    tips: ['onboarding.edit.tip1', 'onboarding.edit.tip2'],
  },
  {
    id: 'upload',
    icon: <Upload className="w-16 h-16 text-green-500" />,
    titleKey: 'onboarding.upload.title',
    descriptionKey: 'onboarding.upload.description',
    tips: ['onboarding.upload.tip1'],
  },
];

const FALLBACKS: Record<string, string> = {
  'onboarding.storage.title': 'Storage Setup',
  'onboarding.storage.description': 'Ensure you have enough disk space for your recordings.',
  'onboarding.storage.tip1': 'Check your available disk space in the Dashboard.',
  'onboarding.storage.tip2': 'You can change the save location in Settings.',
  'onboarding.replay.title': 'Replay Workflow',
  'onboarding.replay.description': 'Download and watch replays to capture specific highlights.',
  'onboarding.replay.tip1': 'Use the Replays tab to find your recent games.',
  'onboarding.replay.tip2': 'Select a target player to record their perspective.',
};

export function OnboardingModal() {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [currentStep, setCurrentStep] = useState(0);

  useEffect(() => {
    // Check if onboarding was already completed
    const completed = localStorage.getItem(ONBOARDING_KEY);
    if (!completed) {
      // Small delay to ensure app is fully loaded
      const timer = setTimeout(() => {
        setIsOpen(true);
      }, 500);
      return () => clearTimeout(timer);
    }
  }, []);

  const handleNext = () => {
    if (currentStep < steps.length - 1) {
      setCurrentStep(currentStep + 1);
    } else {
      handleComplete();
    }
  };

  const handlePrev = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const handleComplete = () => {
    localStorage.setItem(ONBOARDING_KEY, 'true');
    setIsOpen(false);
  };

  const handleSkip = () => {
    localStorage.setItem(ONBOARDING_KEY, 'true');
    setIsOpen(false);
  };

  const step = steps[currentStep];
  const progress = ((currentStep + 1) / steps.length) * 100;
  const isLastStep = currentStep === steps.length - 1;

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm text-muted-foreground">
              {currentStep + 1} / {steps.length}
            </span>
            <Button variant="ghost" size="sm" onClick={handleSkip}>
              {t('onboarding.skip')}
            </Button>
          </div>
          <Progress value={progress} className="h-1 mb-4" />
        </DialogHeader>

        <div className="flex flex-col items-center text-center py-6 space-y-4">
          <div className="p-4 bg-muted/50 rounded-full">
            {step.icon}
          </div>

          <DialogTitle className="text-2xl">
            {t(step.titleKey, FALLBACKS[step.titleKey] ?? step.titleKey)}
          </DialogTitle>

          <DialogDescription className="text-base max-w-sm">
            {t(step.descriptionKey, FALLBACKS[step.descriptionKey] ?? step.descriptionKey)}
          </DialogDescription>

          {step.tips && step.tips.length > 0 && (
            <div className="w-full mt-4 space-y-2">
              {step.tips.map((tipKey, index) => (
                <div
                  key={index}
                  className="flex items-start gap-2 text-sm text-left p-3 bg-muted/30 rounded-lg"
                >
                  <Check className="w-4 h-4 text-green-500 mt-0.5 shrink-0" />
                  <span>{t(tipKey, FALLBACKS[tipKey] ?? tipKey)}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="flex justify-between pt-4">
          <Button
            variant="outline"
            onClick={handlePrev}
            disabled={currentStep === 0}
          >
            <ArrowLeft className="w-4 h-4 mr-2" />
            {t('onboarding.prev')}
          </Button>

          <Button onClick={handleNext}>
            {isLastStep ? (
              <>
                {t('onboarding.getStarted')}
                <Check className="w-4 h-4 ml-2" />
              </>
            ) : (
              <>
                {t('onboarding.next')}
                <ArrowRight className="w-4 h-4 ml-2" />
              </>
            )}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// Hook to manually trigger onboarding
export function useOnboarding() {
  const resetOnboarding = () => {
    localStorage.removeItem(ONBOARDING_KEY);
    window.location.reload();
  };

  const isOnboardingCompleted = () => {
    return localStorage.getItem(ONBOARDING_KEY) === 'true';
  };

  return { resetOnboarding, isOnboardingCompleted };
}
