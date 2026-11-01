#include "ReBoneAnimationSet.h"

int32 UReBoneAnimationSet::FindAnimation(const FString& Animation) const
{
	return Clips.IndexOfByPredicate([&](const FReBoneClip& Clip) { return Clip.Animation == Animation || Clip.Id == Animation; });
}

int32 UReBoneAnimationSet::FindDirection(const FString& ClipName, const FString& InDirection) const
{
	return Clips.IndexOfByPredicate([&](const FReBoneClip& Clip) { return Clip.Name == ClipName && Clip.Direction == InDirection; });
}

FVector UReBoneAnimationSet::PixelToLocal(const FVector2D& Pixel) const
{
	const float Scale = PixelsPerUnit > 0.f ? 1.f / PixelsPerUnit : 1.f;
	return FVector((Pixel.X - Pivot.X) * Scale, 0.f, (Pivot.Y - Pixel.Y) * Scale);
}
