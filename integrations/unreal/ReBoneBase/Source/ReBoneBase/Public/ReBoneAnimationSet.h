// Imported ReBoneBase sprite package: one entry per exported animation.
#pragma once

#include "CoreMinimal.h"
#include "Engine/DataAsset.h"
#include "ReBoneAnimationSet.generated.h"

class UPaperFlipbook;
class USoundBase;

USTRUCT(BlueprintType)
struct FReBoneMarker
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Name;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Payload;
};

/** Collision region in untrimmed source-frame pixels (top-left origin, y down, angle clockwise). */
USTRUCT(BlueprintType)
struct FReBoneRegion
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	int64 Id = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Name;

	/** Hurt, Hit or Interaction. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Kind;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FVector2D Center = FVector2D::ZeroVector;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FVector2D Size = FVector2D::ZeroVector;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	float Angle = 0.f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	TArray<FVector2D> Corners;
};

USTRUCT(BlueprintType)
struct FReBoneFrame
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	int32 SourceFrame = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	int64 SourceTimeMs = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	int32 DurationMs = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	TArray<FReBoneMarker> Markers;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	TArray<FReBoneRegion> Regions;
};

USTRUCT(BlueprintType)
struct FReBoneClip
{
	GENERATED_BODY()

	/** Stable export id such as clip-01. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Id;

	/** Unique animation name, such as Walk or Walk_sw. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Animation;

	/** Authored name shared by every direction of a set. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Name;

	/** Compass id (e, se, s, ...) or empty for a clip without direction. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString Direction;

	/** Degrees clockwise from screen right. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	float DirectionAngle = 0.f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	bool bLooped = true;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	float Fps = 12.f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	TObjectPtr<UPaperFlipbook> Flipbook;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	TArray<FReBoneFrame> Frames;
};

UCLASS(BlueprintType)
class REBONEBASE_API UReBoneAnimationSet : public UDataAsset
{
	GENERATED_BODY()

public:
	/** Export format version the set was imported from. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	int32 FormatVersion = 2;

	/** Untrimmed frame size in pixels. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	FIntPoint FrameSize = FIntPoint::ZeroValue;

	/** Anchor in source-frame pixels; every sprite's pivot is placed here. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	FVector2D Pivot = FVector2D::ZeroVector;

	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	float PixelsPerUnit = 1.f;

	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	TArray<FReBoneClip> Clips;

	/** Imported reference audio, when the package had one. */
	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	TObjectPtr<USoundBase> Audio;

	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	int64 AudioStartMs = 0;

	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	float AudioVolume = 1.f;

	UFUNCTION(BlueprintPure, Category = "ReBoneBase")
	int32 FindAnimation(const FString& Animation) const;

	UFUNCTION(BlueprintPure, Category = "ReBoneBase")
	int32 FindDirection(const FString& ClipName, const FString& InDirection) const;

	/** Region corner in component space: X right, Z up, in Unreal units from the pivot. */
	FVector PixelToLocal(const FVector2D& Pixel) const;
};
