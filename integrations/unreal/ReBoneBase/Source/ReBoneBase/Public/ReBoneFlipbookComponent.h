// Plays a ReBoneBase animation set: flipbooks with exact holds, markers fired
// in frame order, collision regions kept on reused box components, looping
// and direction switching.
#pragma once

#include "CoreMinimal.h"
#include "PaperFlipbookComponent.h"
#include "ReBoneAnimationSet.h"
#include "ReBoneFlipbookComponent.generated.h"

class UBoxComponent;

DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(FReBoneMarkerSignature, const FString&, Name, const FString&, Payload, int32, Frame);
DECLARE_MULTICAST_DELEGATE_ThreeParams(FReBoneMarkerNative, const FString& /*Name*/, const FString& /*Payload*/, int32 /*Frame*/);

UCLASS(ClassGroup = (ReBoneBase), meta = (BlueprintSpawnableComponent))
class REBONEBASE_API UReBoneFlipbookComponent : public UPaperFlipbookComponent
{
	GENERATED_BODY()

public:
	UReBoneFlipbookComponent();

	UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "ReBoneBase")
	TObjectPtr<UReBoneAnimationSet> AnimationSet;

	/** Animation started on BeginPlay; the first clip when empty. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FString InitialAnimation;

	/** Keep one box per collision region and move it as frames change. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	bool bCreateRegions = true;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	FName RegionCollisionProfile = TEXT("OverlapAllDynamic");

	/** Box depth (Y) in Unreal units. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "ReBoneBase")
	float RegionThickness = 8.f;

	/** Fired for every marker of every frame entered, in frame order, including frames passed within one tick. */
	UPROPERTY(BlueprintAssignable, Category = "ReBoneBase")
	FReBoneMarkerSignature OnMarker;

	FReBoneMarkerNative OnMarkerNative;

	/** Start an animation from its first frame. */
	UFUNCTION(BlueprintCallable, Category = "ReBoneBase")
	bool PlayAnimation(const FString& Animation);

	/** Play `ClipName` facing `Direction` (e, se, s, ...). Changing only the direction keeps the current frame and time. */
	UFUNCTION(BlueprintCallable, Category = "ReBoneBase")
	bool PlayDirection(const FString& ClipName, const FString& Direction);

	/** Return to the first frame, firing its markers, and play. */
	UFUNCTION(BlueprintCallable, Category = "ReBoneBase")
	void Restart();

	UFUNCTION(BlueprintPure, Category = "ReBoneBase")
	int32 GetCurrentFrameIndex() const { return FrameIndex; }

	UFUNCTION(BlueprintPure, Category = "ReBoneBase")
	FString GetCurrentAnimation() const;

	/** Region boxes in pool order; only the first GetActiveRegionCount() are in use. */
	const TArray<TObjectPtr<UBoxComponent>>& GetRegionBoxes() const { return RegionBoxes; }

	UFUNCTION(BlueprintPure, Category = "ReBoneBase")
	int32 GetActiveRegionCount() const { return ActiveRegions; }

	/** Advance playback exactly as a tick would. For tests and manual stepping. */
	void Advance(float DeltaSeconds);

	virtual void BeginPlay() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

private:
	const FReBoneClip* CurrentClip() const;
	void Start(int32 Clip, float Position, bool bFireMarkers);
	void Walk(float Before, float Distance);
	void EnterFrame(int32 Index, bool bFireMarkers);
	void ApplyRegions(const FReBoneFrame& Frame);
	float FrameEnd(int32 Index) const;

	int32 ClipIndex = INDEX_NONE;
	int32 FrameIndex = INDEX_NONE;
	int32 ActiveRegions = 0;

	UPROPERTY(Transient)
	TArray<TObjectPtr<UBoxComponent>> RegionBoxes;
};
