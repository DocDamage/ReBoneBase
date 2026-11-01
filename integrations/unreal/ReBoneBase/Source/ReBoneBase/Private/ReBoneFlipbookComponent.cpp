#include "ReBoneFlipbookComponent.h"

#include "Components/AudioComponent.h"
#include "Components/BoxComponent.h"
#include "Sound/SoundBase.h"

namespace
{
	// Targets this close to the track start count as the start; float sums land just below it.
	constexpr float AudioTolerance = 1e-4f;
}
#include "PaperFlipbook.h"

UReBoneFlipbookComponent::UReBoneFlipbookComponent()
{
	PrimaryComponentTick.bCanEverTick = true;
}

const FReBoneClip* UReBoneFlipbookComponent::CurrentClip() const
{
	return AnimationSet && AnimationSet->Clips.IsValidIndex(ClipIndex) ? &AnimationSet->Clips[ClipIndex] : nullptr;
}

FString UReBoneFlipbookComponent::GetCurrentAnimation() const
{
	const FReBoneClip* Clip = CurrentClip();
	return Clip ? Clip->Animation : FString();
}

void UReBoneFlipbookComponent::BeginPlay()
{
	Super::BeginPlay();
	if (AnimationSet && AnimationSet->Clips.Num() > 0 && (InitialAnimation.IsEmpty() || !PlayAnimation(InitialAnimation)))
	{
		Start(0, 0.f, true);
	}
}

bool UReBoneFlipbookComponent::PlayAnimation(const FString& Animation)
{
	const int32 Index = AnimationSet ? AnimationSet->FindAnimation(Animation) : INDEX_NONE;
	if (Index == INDEX_NONE)
	{
		return false;
	}
	Start(Index, 0.f, true);
	return true;
}

bool UReBoneFlipbookComponent::PlayDirection(const FString& ClipName, const FString& Direction)
{
	const int32 Index = AnimationSet ? AnimationSet->FindDirection(ClipName, Direction) : INDEX_NONE;
	if (Index == INDEX_NONE)
	{
		return false;
	}
	const FReBoneClip* Current = CurrentClip();
	const bool bKeep = Current && Current->Name == ClipName && Current->Frames.Num() == AnimationSet->Clips[Index].Frames.Num();
	if (!bKeep)
	{
		Start(Index, 0.f, true);
	}
	else if (Index != ClipIndex)
	{
		// Same animation seen from another side: keep time, do not re-fire the current frame.
		Start(Index, GetPlaybackPosition(), false);
	}
	return true;
}

void UReBoneFlipbookComponent::Restart()
{
	if (CurrentClip())
	{
		Start(ClipIndex, 0.f, true);
	}
}

void UReBoneFlipbookComponent::PausePlayback()
{
	Stop();
	bAudioPaused = true;
	if (AudioComponent)
	{
		AudioComponent->SetPaused(true);
	}
}

void UReBoneFlipbookComponent::ResumePlayback()
{
	Play();
	bAudioPaused = false;
	if (AudioComponent)
	{
		AudioComponent->SetPaused(false);
	}
	SyncAudio(true, FrameIndex);
}

float UReBoneFlipbookComponent::GetAudioTarget() const
{
	const FReBoneClip* Clip = CurrentClip();
	if (!Clip || !AnimationSet || !Clip->Frames.IsValidIndex(FrameIndex) || !Clip->Flipbook)
	{
		return -1.f;
	}
	const float FrameStart = FrameIndex > 0 ? FrameEnd(FrameIndex - 1) : 0.f;
	const float InFrame = FMath::Max(GetPlaybackPosition() - FrameStart, 0.f);
	return Clip->Frames[FrameIndex].SourceTimeMs / 1000.f + InFrame - AnimationSet->AudioStartMs / 1000.f;
}

// Audio runs freely while frames follow each other on the source timeline. It is
// (re)started only where the timeline jumps: play, restart, resume, a loop wrap or
// a frame whose source time does not continue from the previous one.
void UReBoneFlipbookComponent::SyncAudio(bool bForce, int32 Previous)
{
	USoundBase* Sound = AnimationSet ? AnimationSet->Audio.Get() : nullptr;
	if (!bPlayAudio || !Sound)
	{
		return;
	}
	const FReBoneClip* Clip = CurrentClip();
	const float Target = GetAudioTarget();
	const float Length = Sound->GetDuration();
	if (Target < -AudioTolerance || Target >= Length)
	{
		if (AudioComponent)
		{
			AudioComponent->Stop();
		}
		LastAudioStart = -1.f;
		return;
	}
	bool bContinuous = !bForce && LastAudioStart >= 0.f && Clip && Clip->Frames.IsValidIndex(Previous) && Previous != FrameIndex;
	if (bContinuous)
	{
		const FReBoneFrame& Before = Clip->Frames[Previous];
		bContinuous = Before.SourceTimeMs + Before.DurationMs == Clip->Frames[FrameIndex].SourceTimeMs;
	}
	if (bContinuous || !IsPlaying() || bAudioPaused)
	{
		return;
	}
	if (!AudioComponent)
	{
		UObject* Outer = GetOwner() ? static_cast<UObject*>(GetOwner()) : static_cast<UObject*>(this);
		AudioComponent = NewObject<UAudioComponent>(Outer, NAME_None, RF_Transient);
		AudioComponent->bAutoActivate = false;
		if (IsRegistered() && GetWorld())
		{
			AudioComponent->AttachToComponent(this, FAttachmentTransformRules::KeepRelativeTransform);
			AudioComponent->RegisterComponent();
		}
	}
	AudioComponent->SetSound(Sound);
	AudioComponent->SetVolumeMultiplier(AnimationSet->AudioVolume);
	if (AudioComponent->IsRegistered())
	{
		AudioComponent->Play(FMath::Max(Target, 0.f));
	}
	LastAudioStart = FMath::Max(Target, 0.f);
	++AudioStarts;
}

void UReBoneFlipbookComponent::Start(int32 Clip, float Position, bool bFireMarkers)
{
	const bool bWasPlaying = IsPlaying() || ClipIndex == INDEX_NONE;
	ClipIndex = Clip;
	const FReBoneClip& Data = AnimationSet->Clips[Clip];
	SetFlipbook(Data.Flipbook);
	SetLooping(Data.bLooped);
	SetPlaybackPosition(Position, false);
	if (bWasPlaying || bFireMarkers)
	{
		Play();
	}
	FrameIndex = INDEX_NONE;
	bAudioPaused = !IsPlaying();
	const int32 Index = Data.Flipbook ? Data.Flipbook->GetKeyFrameIndexAtTime(Position, true) : 0;
	EnterFrame(FMath::Max(Index, 0), bFireMarkers);
	SyncAudio(true, INDEX_NONE);
}

void UReBoneFlipbookComponent::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	const float Before = AccumulatedTime;
	const bool bForward = bPlaying && !bReversePlayback && PlayRate > 0.f;
	const float Distance = DeltaTime * PlayRate;
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);
	if (bForward)
	{
		Walk(Before, Distance);
	}
	UpdateAudio();
}

void UReBoneFlipbookComponent::Advance(float DeltaSeconds)
{
	const float Before = AccumulatedTime;
	const bool bForward = bPlaying && !bReversePlayback && PlayRate > 0.f;
	const float Distance = DeltaSeconds * PlayRate;
	TickFlipbook(DeltaSeconds);
	if (bForward)
	{
		Walk(Before, Distance);
	}
	UpdateAudio();
}

// Start audio that became due inside a frame, and stop it once the track or the clip ends.
void UReBoneFlipbookComponent::UpdateAudio()
{
	USoundBase* Sound = AnimationSet ? AnimationSet->Audio.Get() : nullptr;
	if (!bPlayAudio || !Sound || bAudioPaused)
	{
		return;
	}
	if (!IsPlaying())
	{
		if (LastAudioStart >= 0.f)
		{
			if (AudioComponent)
			{
				AudioComponent->Stop();
			}
			LastAudioStart = -1.f;
		}
		return;
	}
	const float Target = GetAudioTarget();
	const bool bDue = Target >= -AudioTolerance && Target < Sound->GetDuration();
	if (bDue != (LastAudioStart >= 0.f))
	{
		SyncAudio(true, FrameIndex);
	}
}

float UReBoneFlipbookComponent::FrameEnd(int32 Index) const
{
	const UPaperFlipbook* Flipbook = CurrentClip()->Flipbook;
	int32 Runs = 0;
	for (int32 K = 0; K <= Index; ++K)
	{
		Runs += Flipbook->GetKeyFrameChecked(K).FrameRun;
	}
	return Runs / Flipbook->GetFramesPerSecond();
}

void UReBoneFlipbookComponent::Walk(float Before, float Distance)
{
	const FReBoneClip* Clip = CurrentClip();
	if (!Clip || !Clip->Flipbook || Clip->Flipbook->GetNumKeyFrames() == 0 || Clip->Flipbook->GetTotalDuration() <= 0.f)
	{
		return;
	}
	const UPaperFlipbook* Flipbook = Clip->Flipbook;
	const int32 Count = Flipbook->GetNumKeyFrames();
	constexpr float Tolerance = 1e-5f;
	float Time = Before;
	float Remaining = Distance;
	int32 AtBefore = Count - 1;
	for (int32 K = 0; K < Count; ++K)
	{
		if (Before < FrameEnd(K))
		{
			AtBefore = K;
			break;
		}
	}
	int32 Index = FrameIndex == INDEX_NONE ? AtBefore : FrameIndex;
	if (Index < AtBefore && Clip->bLooped)
	{
		// Within tolerance, the last tick already wrapped to the next loop while the
		// flipbook's clock sits just before the loop end; measure from that new loop.
		Time = Before - Flipbook->GetTotalDuration();
	}
	// Enter every frame whose start lies inside this step, in order, wrapping when looping.
	for (int32 Guard = 0; Remaining > 0.f && Guard < 100000; ++Guard)
	{
		const float End = FrameEnd(Index);
		if (Remaining + Tolerance < End - Time)
		{
			break;
		}
		Remaining -= End - Time;
		Time = End;
		if (Index + 1 < Count)
		{
			++Index;
		}
		else if (Clip->bLooped)
		{
			Index = 0;
			Time = 0.f;
		}
		else
		{
			break;
		}
		EnterFrame(Index, true);
	}
	// The walk above is authoritative. Paper2D's own lookup treats a time exactly on a
	// boundary as the earlier frame, so comparing against it would re-enter frames.
}

void UReBoneFlipbookComponent::EnterFrame(int32 Index, bool bFireMarkers)
{
	const int32 Previous = FrameIndex;
	FrameIndex = Index;
	const FReBoneClip* Clip = CurrentClip();
	if (!Clip || !Clip->Frames.IsValidIndex(Index))
	{
		return;
	}
	const FReBoneFrame& Frame = Clip->Frames[Index];
	if (bFireMarkers)
	{
		for (const FReBoneMarker& Marker : Frame.Markers)
		{
			OnMarker.Broadcast(Marker.Name, Marker.Payload, Index);
			OnMarkerNative.Broadcast(Marker.Name, Marker.Payload, Index);
		}
	}
	ApplyRegions(Frame);
	if (Previous != INDEX_NONE)
	{
		SyncAudio(false, Previous);
	}
}

void UReBoneFlipbookComponent::ApplyRegions(const FReBoneFrame& Frame)
{
	if (!bCreateRegions)
	{
		ActiveRegions = 0;
		return;
	}
	// Grow the pool once; boxes are reused and moved, never recreated per frame.
	while (RegionBoxes.Num() < Frame.Regions.Num())
	{
		UObject* Outer = GetOwner() ? static_cast<UObject*>(GetOwner()) : static_cast<UObject*>(this);
		UBoxComponent* Box = NewObject<UBoxComponent>(Outer, NAME_None, RF_Transient);
		Box->SetCollisionProfileName(RegionCollisionProfile);
		Box->SetGenerateOverlapEvents(true);
		Box->SetHiddenInGame(true);
		if (IsRegistered() && GetWorld())
		{
			Box->AttachToComponent(this, FAttachmentTransformRules::KeepRelativeTransform);
			Box->RegisterComponent();
		}
		else
		{
			Box->SetupAttachment(this);
		}
		RegionBoxes.Add(Box);
	}
	const float Scale = AnimationSet->PixelsPerUnit > 0.f ? 1.f / AnimationSet->PixelsPerUnit : 1.f;
	for (int32 I = 0; I < RegionBoxes.Num(); ++I)
	{
		UBoxComponent* Box = RegionBoxes[I];
		if (!Frame.Regions.IsValidIndex(I))
		{
			Box->SetCollisionEnabled(ECollisionEnabled::NoCollision);
			continue;
		}
		const FReBoneRegion& Region = Frame.Regions[I];
		// Pixels are y-down with clockwise angles; the sprite plane is X right, Z up.
		Box->SetRelativeLocationAndRotation(AnimationSet->PixelToLocal(Region.Center), FRotator(-Region.Angle, 0.f, 0.f));
		Box->SetBoxExtent(FVector(Region.Size.X * 0.5f * Scale, RegionThickness * 0.5f, Region.Size.Y * 0.5f * Scale), false);
		Box->ComponentTags.Reset();
		Box->ComponentTags.Add(FName(*Region.Kind));
		Box->ComponentTags.Add(FName(*Region.Name));
		Box->SetCollisionEnabled(ECollisionEnabled::QueryOnly);
	}
	ActiveRegions = Frame.Regions.Num();
}
