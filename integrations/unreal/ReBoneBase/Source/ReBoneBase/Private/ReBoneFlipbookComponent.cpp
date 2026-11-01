#include "ReBoneFlipbookComponent.h"

#include "Components/BoxComponent.h"
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
	const int32 Index = Data.Flipbook ? Data.Flipbook->GetKeyFrameIndexAtTime(Position, true) : 0;
	EnterFrame(FMath::Max(Index, 0), bFireMarkers);
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
	int32 Index = FrameIndex == INDEX_NONE ? FMath::Max(Flipbook->GetKeyFrameIndexAtTime(Before, true), 0) : FrameIndex;
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
	// Agree with the flipbook's own frame if float rounding disagrees at a boundary.
	const int32 Actual = Flipbook->GetKeyFrameIndexAtTime(GetPlaybackPosition(), true);
	if (Actual != INDEX_NONE && Actual != FrameIndex)
	{
		EnterFrame(Actual, true);
	}
}

void UReBoneFlipbookComponent::EnterFrame(int32 Index, bool bFireMarkers)
{
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
